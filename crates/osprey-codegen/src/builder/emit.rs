//! Emitter emit.
use super::{escape_c_string, Codegen, DebugState, LType, Value, FRAME_POINTER_ATTRS};
use std::fmt::Write as _;

impl Codegen {
    // ---- SSA + block naming (function-local) ----

    pub(crate) fn fresh_reg(&mut self) -> String {
        let r = format!("%r{}", self.reg_count);
        self.reg_count += 1;
        r
    }

    pub(crate) fn fresh_label(&mut self) -> String {
        let l = format!("L{}", self.label_count);
        self.label_count += 1;
        l
    }

    pub(crate) fn cur_block(&self) -> &str {
        &self.cur_block
    }

    // ---- emission ----

    pub(crate) fn emit(&mut self, line: impl Into<String>) {
        let mut line = line.into();
        if let Some(id) = self.debug.as_mut().and_then(DebugState::location_id) {
            let _ = write!(line, ", !dbg !{id}");
        }
        self.cur_lines.push(format!("  {line}"));
    }

    /// Emit `r = {rhs}` to a fresh SSA register and return `r` — the ubiquitous
    /// "name the result of one instruction" step (`zext …`, `icmp …`, `fneg …`).
    /// Open a diamond on `cond`: mint the two arm labels plus the join they
    /// both reach, and emit the branch between them. Answers
    /// `(true_arm, false_arm, join)`; the caller starts whichever arm it means
    /// to fill first. Minting labels apart from the `br` that names them is how
    /// a block ends up unterminated, so the two steps are one call.
    pub(crate) fn diamond(&mut self, cond: &str) -> (String, String, String) {
        let (taken, other, join) = (self.fresh_label(), self.fresh_label(), self.fresh_label());
        self.emit(format!("br i1 {cond}, label %{taken}, label %{other}"));
        (taken, other, join)
    }

    pub(crate) fn emit_reg(&mut self, rhs: impl std::fmt::Display) -> String {
        let r = self.fresh_reg();
        self.emit(format!("{r} = {rhs}"));
        r
    }

    /// Start a new basic block and make it current (its label becomes the
    /// predecessor recorded for any `phi` that follows).
    pub(crate) fn start_block(&mut self, label: &str) {
        self.cur_lines.push(format!("{label}:"));
        self.cur_block = label.to_string();
    }

    /// Snapshot the current block label, then branch to `end` — the predecessor
    /// a `phi` at `end` reads back. Closes a one-arm path of a Result/match split.
    pub(crate) fn snapshot_to(&mut self, end: &str) -> String {
        let block = self.cur_block.clone();
        self.emit(format!("br label %{end}"));
        block
    }

    pub(crate) fn add_extern(&mut self, decl: impl Into<String>) {
        let _ = self.externs.insert(decl.into());
    }

    /// Append a module-level global definition: closure cells, counter
    /// globals, the fiber-result table.
    pub(crate) fn add_global(&mut self, def: impl Into<String>) {
        self.globals.push(def.into());
    }

    /// The shared immortal handle for a nullary union variant with tag `tag`,
    /// interning it on first use. The `private global` bakes the exact
    /// `{ i64 meta, i32 rc, u32 size }` ARC header (`memory_arc.c`) ahead of a
    /// one-word `{ i64 tag }` body: rc = -1 makes every dup/drop a no-op, so
    /// the block is never freed and needs no registry slot. The returned
    /// operand is a constant `getelementptr`+`bitcast` to the body — not a
    /// `%`-register — so `arc::managed` treats it as borrowed and emits no
    /// retain/release around it, exactly the immortal contract. [GC-ARC-PERCEUS]
    pub(crate) fn nullary_singleton(&mut self, name: &str, tag: i64) -> String {
        if let Some(handle) = self.nullary_singletons.get(name) {
            return handle.clone();
        }
        // meta = KIND_RAW (a tag-only body has no managed children); rc = -1
        // (immortal); size = 8 (the lone i64 tag). Layout matches OspArcHdr so
        // arc_hdr(body) = body - 16 recovers these fields.
        let global = format!("@{name}.sgl");
        let ty = "{ i64, i32, i32, i64 }";
        self.globals.push(format!(
            "{global} = private global {ty} {{ i64 {raw}, i32 -1, i32 8, i64 {tag} }}, align 16",
            raw = crate::meta::KIND_RAW
        ));
        let handle = format!(
            "bitcast (i64* getelementptr inbounds ({ty}, {ty}* {global}, i64 0, i32 3) to i8*)"
        );
        let _ = self
            .nullary_singletons
            .insert(name.to_string(), handle.clone());
        handle
    }

    /// Intern a string literal as a private global and return an `i8*` pointing
    /// at its first byte.
    pub(crate) fn string_constant(&mut self, text: &str) -> Value {
        let (escaped, len) = escape_c_string(text);
        let name = format!("@.str.{}", self.glob_count);
        self.glob_count += 1;
        self.globals.push(format!(
            "{name} = private unnamed_addr constant [{len} x i8] c\"{escaped}\""
        ));
        let reg = self.emit_reg(format!(
            "getelementptr [{len} x i8], [{len} x i8]* {name}, i64 0, i64 0"
        ));
        let _ = self.rodata_regs.insert(reg.clone());
        Value::new(reg, LType::Str)
    }

    /// Assemble the final module text: header, externals, globals, functions.
    pub(crate) fn render(&self) -> String {
        let mut out = String::from("; Generated by osprey-rs (Rust LLVM-text backend)\n\n");
        if let Some(debug) = &self.debug {
            let _ = write!(out, "source_filename = \"{}\"\n\n", debug.source_filename());
        }
        for decl in &self.externs {
            out.push_str(decl);
            out.push('\n');
        }
        out.push('\n');
        for g in &self.globals {
            out.push_str(g);
            out.push('\n');
        }
        out.push('\n');
        out.push_str(&self.funcs.join("\n\n"));
        out.push('\n');
        if let Some(init) = self.cov_render_init() {
            out.push('\n');
            out.push_str(&init);
            out.push('\n');
        }
        out.push_str(FRAME_POINTER_ATTRS);
        out.push('\n');
        if let Some(debug) = &self.debug {
            out.push('\n');
            out.push_str(&debug.compile_units());
            out.push('\n');
            out.push_str(&debug.module_flags());
            out.push('\n');
            out.push_str(&debug.ident());
            out.push('\n');
            for line in debug.metadata_lines() {
                out.push_str(&line);
                out.push('\n');
            }
        }
        out
    }
}
