use std::{fs, path::PathBuf, process::Command};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::args().nth(1).ok_or("matrix root")?);
    let out = root.parent().ok_or("parent")?.join("runtime");
    fs::create_dir_all(&out)?;
    let mut receipt = String::new();
    for subject in ["effect", "return"] {
        let source = fs::read_to_string(root.join(format!("{subject}_assert_ne_direct/src/lib.rs")))?;
        let production = source.split_once("#[cfg(test)]").ok_or("production")?.0;
        let mut tests = String::new();
        for name in ["assert_ne", "debug_assert_eq", "debug_assert", "debug_assert_ne"] {
            for context in ["direct", "block_return", "fn_uncalled"] {
                let source = fs::read_to_string(root.join(format!("{subject}_{name}_{context}/src/lib.rs")))?;
                let test = source.split_once("#[cfg(test)]").ok_or("test")?.1;
                tests.push_str("#[cfg(test)]");
                tests.push_str(&test.replacen("mod tests", &format!("mod tests_{name}_{context}"), 1));
            }
        }
        for version in ["correct", "wrong"] {
            let production = if version == "correct" { production.to_owned() }
                else if subject == "effect" { production.replace("notifier.send(order_id)", "notifier.send(\"static_msg\")") }
                else { production.replace("base * 2", "base + 1") };
            let stem = format!("{subject}_{version}");
            let src = out.join(format!("{stem}.rs"));
            let bin = out.join(&stem);
            fs::write(&src, format!("{production}{tests}"))?;
            let compile = Command::new("rustc").arg("--edition=2024").arg("--test").arg("-C").arg("debug-assertions=yes").arg(&src).arg("-o").arg(&bin).output()?;
            fs::write(out.join(format!("{stem}.compile.stderr")), &compile.stderr)?;
            assert!(compile.status.success(), "{stem} compile");
            let listed = Command::new(&bin).arg("--list").output()?;
            fs::write(out.join(format!("{stem}.list.stdout")), &listed.stdout)?;
            let listing = String::from_utf8(listed.stdout)?;
            assert!(listed.status.success() && listing.contains("12 tests, 0 benchmarks"));
            for name in ["assert_ne", "debug_assert_eq", "debug_assert", "debug_assert_ne"] {
                for context in ["direct", "block_return", "fn_uncalled"] {
                    let test_name = if subject == "effect" { "process_order_observed" } else { "compute_score_observed" };
                    let exact = format!("tests_{name}_{context}::{test_name}");
                    let run = Command::new(&bin).arg("--exact").arg(&exact).arg("--nocapture").output()?;
                    let key = format!("{stem}_{name}_{context}");
                    fs::write(out.join(format!("{key}.stdout")), &run.stdout)?;
                    fs::write(out.join(format!("{key}.stderr")), &run.stderr)?;
                    assert!(String::from_utf8_lossy(&run.stdout).contains("running 1 test"), "{key}: no execution");
                    let expected_pass = version == "correct" || context != "direct";
                    receipt.push_str(&format!("{key}: expected_pass={expected_pass}, exit={}\n", run.status.code().unwrap_or(-1)));
                    assert_eq!(run.status.success(), expected_pass, "{key}");
                }
            }
        }
    }
    fs::write(out.join("receipt.txt"), &receipt)?;
    print!("{receipt}");
    Ok(())
}
