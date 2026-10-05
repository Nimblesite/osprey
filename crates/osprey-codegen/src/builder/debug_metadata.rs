//! LLVM/DWARF identities and source metadata.
use super::{host_dwarf_version, metadata_escape, DebugSource, LType, Position};

#[derive(Debug, Clone)]
pub(super) struct DebugState {
    pub(super) source: DebugSource,
    pub(super) current_scope: Option<usize>,
    pub(super) current_function: Option<usize>,
    pub(super) current_position: Option<Position>,
    pub(super) current_retained_nodes: Option<usize>,
    pub(super) current_local_ids: Vec<usize>,
    pub(super) next_id: usize,
    pub(super) file_id: usize,
    pub(super) cu_id: usize,
    pub(super) empty_id: usize,
    pub(super) subroutine_type_id: usize,
    pub(super) dwarf_flag_id: usize,
    pub(super) debug_version_flag_id: usize,
    pub(super) ident_id: usize,
    pub(super) dwarf_version: u8,
    pub(super) i64_type_id: usize,
    pub(super) i32_type_id: usize,
    pub(super) bool_type_id: usize,
    pub(super) double_type_id: usize,
    pub(super) char_type_id: usize,
    pub(super) ptr_type_id: usize,
    pub(super) record_types: std::collections::HashMap<String, usize>,
    pub(super) opaque_ptr_type_id: usize,
    pub(super) dynamic: Vec<(usize, String)>,
}

impl DebugState {
    pub(super) fn new(source: DebugSource) -> Self {
        DebugState {
            source,
            current_scope: None,
            current_function: None,
            current_position: None,
            current_retained_nodes: None,
            current_local_ids: Vec::new(),
            next_id: 14,
            file_id: 0,
            cu_id: 1,
            empty_id: 2,
            subroutine_type_id: 3,
            dwarf_flag_id: 4,
            debug_version_flag_id: 5,
            ident_id: 6,
            dwarf_version: host_dwarf_version(),
            i64_type_id: 7,
            i32_type_id: 8,
            bool_type_id: 9,
            double_type_id: 10,
            char_type_id: 11,
            ptr_type_id: 12,
            opaque_ptr_type_id: 13,
            record_types: std::collections::HashMap::new(),
            dynamic: Vec::new(),
        }
    }

    pub(super) fn source_filename(&self) -> String {
        metadata_escape(&self.source.path().display().to_string())
    }

    pub(super) fn begin_function(&mut self, name: &str, position: Option<Position>) -> usize {
        let retained_id = self.alloc_id();
        let id = self.alloc_id();
        let line = position.map_or(1, |p| p.line.max(1));
        // [MODULES-ABI] LLDB prefers linkageName even when it cannot decode
        // our encoding. Keep DWARF names readable; native LLVM symbols keep ABI.
        let name = metadata_escape(&osprey_ast::symbol::demangle_message(name));
        self.dynamic.push((
            id,
            format!(
                "!{id} = distinct !DISubprogram(name: \"{name}\", scope: !{}, file: !{}, line: {line}, type: !{}, scopeLine: {line}, spFlags: DISPFlagDefinition, unit: !{}, retainedNodes: !{})",
                self.file_id,
                self.file_id,
                self.subroutine_type_id,
                self.cu_id,
                retained_id
            ),
        ));
        self.current_scope = Some(id);
        self.current_function = Some(id);
        self.current_position = position;
        self.current_retained_nodes = Some(retained_id);
        self.current_local_ids.clear();
        id
    }

    pub(super) fn finish_function(&mut self) {
        if let Some(id) = self.current_retained_nodes.take() {
            let locals = self
                .current_local_ids
                .iter()
                .map(|local| format!("!{local}"))
                .collect::<Vec<_>>()
                .join(", ");
            self.dynamic.push((id, format!("!{id} = !{{{locals}}}")));
        }
        self.current_local_ids.clear();
    }

    pub(super) fn clear_function(&mut self) {
        self.current_scope = None;
        self.current_function = None;
        self.current_position = None;
        self.current_retained_nodes = None;
        self.current_local_ids.clear();
    }

    pub(super) fn set_position(&mut self, position: Option<Position>) -> Option<Position> {
        let previous = self.current_position;
        if position.is_some() {
            self.current_position = position;
        }
        previous
    }

    pub(super) fn location_id(&mut self) -> Option<usize> {
        let scope = self.current_scope?;
        let position = self.current_position?;
        let id = self.alloc_id();
        let line = position.line.max(1);
        let column = position.column.saturating_add(1).max(1);
        self.dynamic.push((
            id,
            format!("!{id} = !DILocation(line: {line}, column: {column}, scope: !{scope})"),
        ));
        Some(id)
    }

    pub(super) fn local_variable_id(
        &mut self,
        name: &str,
        type_id: usize,
        argument: Option<usize>,
    ) -> Option<usize> {
        let scope = self.current_scope?;
        let position = self.current_position?;
        let id = self.alloc_id();
        let line = position.line.max(1);
        let name = metadata_escape(name);
        let argument = argument.map_or_else(String::new, |index| format!("arg: {index}, "));
        self.dynamic.push((
            id,
            format!(
                "!{id} = !DILocalVariable(name: \"{name}\", {argument}scope: !{scope}, file: !{}, line: {line}, type: !{type_id})",
                self.file_id
            ),
        ));
        self.current_local_ids.push(id);
        Some(id)
    }

    pub(super) fn debug_type_id(&self, ty: LType) -> usize {
        match ty {
            LType::I64 => self.i64_type_id,
            LType::I32 => self.i32_type_id,
            LType::I1 => self.bool_type_id,
            LType::Double => self.double_type_id,
            LType::Str => self.ptr_type_id,
            LType::Ptr | LType::Any => self.opaque_ptr_type_id,
        }
    }

    pub(super) fn metadata_lines(&self) -> Vec<String> {
        let file = metadata_escape(&self.source.filename);
        let dir = metadata_escape(&self.source.directory);
        let mut out = vec![
            format!(
                "!{} = !DIFile(filename: \"{file}\", directory: \"{dir}\")",
                self.file_id
            ),
            format!(
                "!{} = distinct !DICompileUnit(language: DW_LANG_C, file: !{}, producer: \"osprey\", isOptimized: false, runtimeVersion: 0, emissionKind: FullDebug)",
                self.cu_id, self.file_id
            ),
            format!("!{} = !{{}}", self.empty_id),
            format!(
                "!{} = !DISubroutineType(types: !{})",
                self.subroutine_type_id, self.empty_id
            ),
            format!(
                "!{} = !{{i32 2, !\"Dwarf Version\", i32 {}}}",
                self.dwarf_flag_id, self.dwarf_version
            ),
            format!(
                "!{} = !{{i32 2, !\"Debug Info Version\", i32 3}}",
                self.debug_version_flag_id
            ),
            format!("!{} = !{{!\"osprey\"}}", self.ident_id),
            format!(
                "!{} = !DIBasicType(name: \"int\", size: 64, encoding: DW_ATE_signed)",
                self.i64_type_id
            ),
            format!(
                "!{} = !DIBasicType(name: \"c_int\", size: 32, encoding: DW_ATE_signed)",
                self.i32_type_id
            ),
            format!(
                "!{} = !DIBasicType(name: \"bool\", size: 8, encoding: DW_ATE_boolean)",
                self.bool_type_id
            ),
            format!(
                "!{} = !DIBasicType(name: \"float\", size: 64, encoding: DW_ATE_float)",
                self.double_type_id
            ),
            format!(
                "!{} = !DIBasicType(name: \"char\", size: 8, encoding: DW_ATE_signed_char)",
                self.char_type_id
            ),
            format!(
                "!{} = !DIDerivedType(tag: DW_TAG_pointer_type, baseType: !{}, size: 64)",
                self.ptr_type_id, self.char_type_id
            ),
        ];
        out.push(format!(
            "!{} = !DIDerivedType(tag: DW_TAG_pointer_type, baseType: null, size: 64)",
            self.opaque_ptr_type_id
        ));
        out.extend(self.dynamic.iter().map(|(_, line)| line.clone()));
        out
    }

    pub(super) fn module_flags(&self) -> String {
        format!(
            "!llvm.module.flags = !{{!{}, !{}}}",
            self.dwarf_flag_id, self.debug_version_flag_id
        )
    }

    pub(super) fn compile_units(&self) -> String {
        format!("!llvm.dbg.cu = !{{!{}}}", self.cu_id)
    }

    pub(super) fn ident(&self) -> String {
        format!("!llvm.ident = !{{!{}}}", self.ident_id)
    }

    pub(super) fn alloc_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}
