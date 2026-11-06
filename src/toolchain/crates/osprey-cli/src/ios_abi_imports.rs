//! Checked host thunks with owned UTF-8 inputs and results. [IOS-HOST-IMPORTS]
use super::{
    import_adapter_name, llvm_params, rename_symbol, CType, Export, Import, HOST_STRING_COPY,
};

pub(super) fn adapt_import(ir: &str, import: &Import, extend_bool: bool) -> String {
    let needle = format!("@{}(", import.symbol);
    let Some((line_index, _)) = ir
        .split_inclusive('\n')
        .enumerate()
        .find(|(_, line)| line.starts_with("declare ") && line.contains(&needle))
    else {
        return ir.to_string();
    };
    let adapter = import_adapter_name(import);
    let renamed = rename_symbol(&without_line(ir, line_index), &import.symbol, &adapter);
    format!(
        "{renamed}\n{}\n{}",
        declaration_for(import, extend_bool),
        thunk(import, &adapter, extend_bool)
    )
}

fn without_line(ir: &str, removed: usize) -> String {
    ir.split_inclusive('\n')
        .enumerate()
        .filter_map(|(index, line)| (index != removed).then_some(line))
        .collect()
}

fn declaration_for(import: &Import, extend_bool: bool) -> String {
    let params = import
        .params
        .iter()
        .map(|(_, c)| {
            format!("{} {}", c.llvm(), c.zeroext(extend_bool))
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "declare {}{} @{}({params})",
        import.ret.zeroext(extend_bool),
        import.ret.llvm(),
        import.symbol
    )
}

fn thunk(import: &Import, adapter: &str, extend_bool: bool) -> String {
    let internal = llvm_params(&import.params, false);
    let boundary = llvm_params(&import.params, extend_bool);
    let ret = import.ret;
    let call = format!(
        "call {}{} @{}({boundary})",
        ret.zeroext(extend_bool),
        ret.llvm(),
        import.symbol
    );
    let body = returned(ret, &call);
    format!(
        "\ndefine internal {} @{adapter}({internal}) {{\n{body}\n}}\n",
        ret.llvm_internal()
    )
}

fn returned(ret: CType, call: &str) -> String {
    match ret {
        CType::Unit => format!("  {call}\n  ret i64 0"),
        CType::Str => format!(
            "  %r = {call}\n  %owned = call i8* @{HOST_STRING_COPY}(i8* %r)\n  ret i8* %owned"
        ),
        other => format!("  %r = {call}\n  ret {} %r", other.llvm()),
    }
}

pub(super) fn export_thunk(export: &Export, extend_bool: bool) -> String {
    let boundary = llvm_params(&export.params, extend_bool);
    let forwarded = forwarded_inputs(&export.params);
    let call = format!(
        "call {} @{}({forwarded})",
        export.ret.llvm_internal(),
        export.symbol
    );
    let body = export_body(export, &call);
    format!(
        "\ndefine {}{} @{}({boundary}) {{\n{body}\n}}\n",
        export.ret.zeroext(extend_bool),
        export.ret.llvm(),
        export.c_name
    )
}

fn forwarded_inputs(params: &[(String, CType)]) -> String {
    params
        .iter()
        .enumerate()
        .map(|(i, (_, ty))| {
            let name = if *ty == CType::Str {
                format!("owned_p{i}")
            } else {
                format!("p{i}")
            };
            format!("{} %{name}", ty.llvm())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn export_body(export: &Export, call: &str) -> String {
    // Host arguments live only for this call. The Osprey callee can retain
    // registered copies when returning them or storing them. [IOS-HOST-ABI]
    let copies = string_inputs(&export.params, |i| {
        format!("  %owned_p{i} = call i8* @{HOST_STRING_COPY}(i8* %p{i})\n")
    });
    let releases = string_inputs(&export.params, |i| {
        format!("  call void @osp_release(i8* %owned_p{i})\n")
    });
    if export.ret == CType::Unit {
        format!("{copies}  {call}\n{releases}  ret void")
    } else {
        format!(
            "{copies}  %r = {call}\n{releases}  ret {} %r",
            export.ret.llvm()
        )
    }
}

fn string_inputs(params: &[(String, CType)], emit: impl Fn(usize) -> String) -> String {
    params
        .iter()
        .enumerate()
        .filter(|(_, (_, ty))| *ty == CType::Str)
        .map(|(i, _)| emit(i))
        .collect()
}
