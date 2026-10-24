//! Native controls for a physical file compiled by independent Cargo roots.
use super::*;
use std::ffi::OsStr;

#[test]
fn shared_target_macro_context_matches_executed_binary_controls() -> Result<(), String> {
    let rustc = std::env::var_os("RUSTC")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("rustc"));
    for effect in [false, true] {
        for shadowed in [false, true] {
            let scratch = Scratch::create()?;
            std::fs::create_dir(scratch.0.join("src")).map_err(|e| e.to_string())?;
            std::fs::write(scratch.0.join("Cargo.toml"), "[package]\nname=\"shared_context\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[lib]\ntest=false\ndoctest=false\n").map_err(|e| e.to_string())?;
            std::fs::write(scratch.0.join("src/lib.rs"), "mod shared;\n")
                .map_err(|e| e.to_string())?;
            let shadow = if shadowed {
                "macro_rules! assert_eq { ($($ignored:tt)*) => {} }\n"
            } else {
                ""
            };
            let main = scratch.0.join("src/main.rs");
            std::fs::write(&main, format!("{shadow}mod shared;\nfn main() {{}}\n"))
                .map_err(|e| e.to_string())?;
            let (good, wrong, assertion) = if effect {
                (
                    "pub fn owner(out: &mut Vec<u32>) {\n    out.push(3);\n}\n",
                    "pub fn owner(out: &mut Vec<u32>) {\n    out.push(4);\n}\n",
                    "let mut out = Vec::new();\nowner(&mut out);\nassert_eq!(out, Vec::from([3]));",
                )
            } else {
                (
                    "pub fn owner(x: u32) -> u32 {\n    3 * x\n}\n",
                    "pub fn owner(x: u32) -> u32 {\n    4 * x\n}\n",
                    "assert_eq!(owner(2), 6);",
                )
            };
            let tail = format!(
                "#[cfg(test)]\nmod tests {{\nuse super::*;\n#[test]\nfn pin() {{\n{assertion}\n}}\n}}\n"
            );
            let source = scratch.0.join("src/shared.rs");
            std::fs::write(&source, format!("{good}{tail}")).map_err(|e| e.to_string())?;
            let diff = scratch.0.join("diff.patch");
            std::fs::write(&diff, format!("--- a/src/shared.rs\n+++ b/src/shared.rs\n@@ -1,3 +1,3 @@\n {}\n-{}\n+{}\n }}\n", good.lines().next().ok_or("missing owner")?, wrong.lines().nth(1).ok_or("missing wrong line")?, good.lines().nth(1).ok_or("missing correct line")?)).map_err(|e| e.to_string())?;
            let report = check_workspace(CheckInput {
                root: scratch.0.clone(),
                diff_file: Some(diff),
                mode: Mode::Fast,
                include_unchanged_tests: true,
                ..CheckInput::default()
            })?;
            let selected: Vec<_> = report
                .findings
                .iter()
                .filter(|f| {
                    if effect {
                        matches!(
                            f.probe.family,
                            ProbeFamily::SideEffect | ProbeFamily::CallDeletion
                        )
                    } else {
                        f.probe.family == ProbeFamily::ReturnValue
                    }
                })
                .collect();
            assert_eq!(
                selected.len(),
                if effect { 2 } else { 1 },
                "nonempty intended family subjects"
            );
            for finding in selected {
                assert_eq!(
                    finding.class == ExposureClass::Exposed,
                    !shadowed,
                    "effect={effect}; shadowed={shadowed}: {finding:?}"
                );
            }
            for (label, production, should_pass) in
                [("correct", good, true), ("wrong_value", wrong, shadowed)]
            {
                std::fs::write(&source, format!("{production}{tail}"))
                    .map_err(|e| e.to_string())?;
                let executable = scratch
                    .0
                    .join(format!("{label}{}", std::env::consts::EXE_SUFFIX));
                compiles(
                    run(
                        &rustc,
                        &[
                            OsStr::new("--edition=2024"),
                            OsStr::new("--test"),
                            main.as_os_str(),
                            OsStr::new("-o"),
                            executable.as_os_str(),
                        ],
                    )?,
                    label,
                )?;
                let result = run(&executable, &[OsStr::new("--nocapture")])?;
                assert!(
                    String::from_utf8_lossy(&result.stdout).contains("running 1 test"),
                    "the binary copy's test must execute"
                );
                assert_eq!(
                    result.status.success(),
                    should_pass,
                    "effect={effect}; shadowed={shadowed}; {label}: {}",
                    String::from_utf8_lossy(&result.stdout)
                );
            }
        }
    }
    Ok(())
}
