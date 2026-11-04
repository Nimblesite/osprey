use super::*;
use crate::ios_abi::ExportScope;

#[test]
fn target_slices_match_ndk_and_bool_abi() -> Result<(), String> {
    let parsed = osprey_syntax::parse_program(
        "extern fn host_flag(value: bool) -> bool\nfn flag(value: bool) = host_flag(value)\n",
    );
    for (name, target, prefix) in [
        ("android-arm64", Target::Arm64, "i1"),
        ("android-x64", Target::X64, "zeroext i1"),
    ] {
        assert_eq!(Target::parse(name), Some(target));
        let (ir, header) = source(
            &parsed.program,
            "flag.osp",
            target,
            &ExportScope::EntrySource(None),
        )?;
        assert!(
            ir.contains(&format!("define {prefix} @osprey_flag(")),
            "{ir}"
        );
        assert!(
            ir.contains(&format!("declare {prefix} @host_flag(")),
            "{ir}"
        );
        assert!(header.contains(&format!("--target={name}")));
        crate::testkit::shows(&header, &["bool osprey_flag(bool value)"]);
        assert!(compile_args(target, Path::new("a.ll"), Path::new("a.o"))
            .contains(&"-fPIC".to_string()));
    }
    assert_ne!(Target::Arm64.runtime("arc"), Target::X64.runtime("arc"));
    Ok(())
}

#[test]
fn unsupported_android_options_are_errors_without_ndk() -> Result<(), String> {
    for target in ["android-arm64", "android-x64"] {
        let accepted = [
            "app.osp",
            &format!("--target={target}"),
            "--compile",
            "--memory=arc",
        ]
        .map(str::to_string);
        let cli = crate::parse_args(&accepted)?;
        assert_eq!(cli.memory, "arc");
        assert!(validate(&cli).is_ok());
        for option in ["--memory=gc", "--debug", "--profile", "--run"] {
            let args = [
                "app.osp".to_string(),
                format!("--target={target}"),
                option.to_string(),
            ];
            assert!(validate(&crate::parse_args(&args)?).is_err());
        }
    }
    Ok(())
}
