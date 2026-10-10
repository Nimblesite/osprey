//! Emitter allocation.
use super::{Codegen, OSP_ALLOC_DECL, OSP_ALLOC_TAGGED_DECL, OSP_ALLOC_TAGGED_NOINIT_DECL};

impl Codegen {
    /// Allocate `size` bytes through the swappable Osprey allocation hook and
    /// return the raw `i8*` register. The single heap-allocation primitive every
    /// codegen site funnels through, so the memory backend is chosen at link time
    /// (default `osp_alloc` = `malloc`; ARC / tracing-GC / arena swap in behind
    /// the same symbol) — never baked into the IR. Implements [MEM-BACKENDS],
    /// docs/specs/0018. The allocator attributes are load-bearing: they let LLVM
    /// recognise `@osp_alloc` as an allocation function, so `-O2` proves
    /// non-escaping allocations dead and removes them entirely (the
    /// [MEM-OWNERSHIP] static free-at-last-use, achieved by the optimizer).
    pub(crate) fn heap_alloc(&mut self, size: &str) -> String {
        self.add_extern(OSP_ALLOC_DECL);
        self.emit_reg(format!("call i8* @osp_alloc(i64 {size})"))
    }

    /// [`heap_alloc`] carrying the per-site layout word (kind + managed-pointer
    /// mask, see [`crate::meta`]): the ARC backend stores it in the object
    /// header so `osp_release` can drop children precisely; other backends
    /// ignore it. Implements [GC-ARC-PERCEUS].
    pub(super) fn heap_alloc_tagged(&mut self, size: &str, meta: i64) -> String {
        self.heap_alloc_tagged_via(size, meta, OSP_ALLOC_TAGGED_DECL, "osp_alloc_tagged")
    }

    /// [`heap_alloc_tagged`] for a block the caller fully initializes before it
    /// can drop (every masked word stored): the ARC backend skips its
    /// drop-safety pre-zero. A `KIND_RAW` block is never zeroed anyway, so it
    /// shares the plain allocator. [GC-ARC-PERCEUS]
    pub(super) fn heap_alloc_tagged_noinit(&mut self, size: &str, meta: i64) -> String {
        self.heap_alloc_tagged_via(
            size,
            meta,
            OSP_ALLOC_TAGGED_NOINIT_DECL,
            "osp_alloc_tagged_noinit",
        )
    }

    /// Shared body of the tagged allocators: a `KIND_RAW` block has nothing to
    /// mark so it falls back to the plain allocator; otherwise declare `decl` and
    /// emit `call i8* @{func}(i64 size, i64 meta)`.
    pub(super) fn heap_alloc_tagged_via(
        &mut self,
        size: &str,
        meta: i64,
        decl: &str,
        func: &str,
    ) -> String {
        if meta == crate::meta::KIND_RAW {
            return self.heap_alloc(size);
        }
        self.add_extern(decl);
        self.emit_reg(format!("call i8* @{func}(i64 {size}, i64 {meta})"))
    }

    /// Allocate a heap block sized for the LLVM struct type `struct_ty`, via the
    /// portable `getelementptr null, 1` sizeof trick, and return the typed
    /// pointer register (`{TY}*`). `meta` is the site's layout word
    /// ([`crate::meta`]).
    pub(crate) fn malloc_struct(&mut self, struct_ty: &str, meta: i64) -> String {
        self.malloc_struct_with(struct_ty, meta, false)
    }

    /// [`malloc_struct`] for a block the caller stores in full before it can
    /// drop — a constructor / object literal writes the tag and every field —
    /// so the ARC backend skips its drop-safety pre-zero. [GC-ARC-PERCEUS]
    pub(crate) fn malloc_struct_noinit(&mut self, struct_ty: &str, meta: i64) -> String {
        self.malloc_struct_with(struct_ty, meta, true)
    }

    pub(super) fn malloc_struct_with(
        &mut self,
        struct_ty: &str,
        meta: i64,
        noinit: bool,
    ) -> String {
        let szp = self.emit_reg(format!(
            "getelementptr {struct_ty}, {struct_ty}* null, i64 1"
        ));
        let sz = self.emit_reg(format!("ptrtoint {struct_ty}* {szp} to i64"));
        let raw = if noinit {
            self.heap_alloc_tagged_noinit(&sz, meta)
        } else {
            self.heap_alloc_tagged(&sz, meta)
        };
        let obj = self.emit_reg(format!("bitcast i8* {raw} to {struct_ty}*"));
        obj
    }

    /// Hoist a null-initialized `i8*` spill slot into the entry block and
    /// return its register. ARC region drops load these slots, so a drop is
    /// valid from any block and untaken paths release `null` (a no-op) —
    /// ownership without dominance analysis [GC-ARC-PERCEUS].
    /// Whether `reg` addresses rodata (a string-literal global), so ARC
    /// dup/drop on it is provably a no-op [GC-ARC-PERCEUS].
    pub(crate) fn is_rodata(&self, reg: &str) -> bool {
        self.rodata_regs.contains(reg)
    }

    pub(crate) fn hoist_arc_slot(&mut self) -> String {
        let name = format!("%arc.s{}", self.arc_slot_count);
        self.arc_slot_count += 1;
        self.cur_lines.insert(1, format!("  {name} = alloca i8*"));
        self.cur_lines
            .insert(2, format!("  store i8* null, i8** {name}"));
        name
    }
}
