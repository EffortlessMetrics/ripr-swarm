use std::process::{Command, Output};

fn ripr(args: &[&str]) -> Result<Output, std::io::Error> {
    Command::new(env!("CARGO_BIN_EXE_ripr")).args(args).output()
}

fn assert_usage_refusal(output: Output, expected: &str) {
    assert_eq!(output.status.code(), Some(2));
    assert!(
        output.stdout.is_empty(),
        "usage refusal must keep stdout empty: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!("ripr: {expected}\n")
    );
}

#[test]
fn agent_brief_numeric_refusals_name_the_exact_cli_value() -> Result<(), Box<dyn std::error::Error>>
{
    for value in ["0", "-1", "nonnumeric", "18446744073709551616"] {
        assert_usage_refusal(
            ripr(&[
                "agent",
                "brief",
                "--root",
                "/tmp/ripr-8835-missing-root",
                "--diff",
                "/tmp/ripr-8835-missing.diff",
                "--json",
                "--max-seams",
                value,
            ])?,
            &format!("--max-seams requires a positive integer; got {value:?}"),
        );
    }

    assert_usage_refusal(
        ripr(&[
            "agent",
            "brief",
            "--root",
            "/tmp/ripr-8835-missing-root",
            "--diff",
            "/tmp/ripr-8835-missing.diff",
            "--json",
            "--max-seams",
            "11",
        ])?,
        "invalid --max-seams: maximum is 10; got \"11\"",
    );

    assert_usage_refusal(
        ripr(&[
            "agent",
            "brief",
            "--diff",
            "/tmp/ripr-8835-missing.diff",
            "--json",
            "--max-seams",
            "0",
            "--max-seams",
            "1",
        ])?,
        "--max-seams requires a positive integer; got \"0\"",
    );
    Ok(())
}

#[test]
fn agent_verify_execute_numeric_refusals_name_the_exact_cli_value()
-> Result<(), Box<dyn std::error::Error>> {
    for value in ["0", "-1", "nonnumeric", "18446744073709551616"] {
        assert_usage_refusal(
            ripr(&[
                "agent",
                "verify-execute",
                "--root",
                "/tmp/ripr-8835-missing-root",
                "--packet",
                "/tmp/ripr-8835-missing-packet.json",
                "--result-json",
                "/tmp/ripr-8835-result.json",
                "--json",
                "--cancel-after-ms",
                value,
            ])?,
            &format!("--cancel-after-ms requires a positive integer; got {value:?}"),
        );
    }

    assert_usage_refusal(
        ripr(&[
            "agent",
            "verify-execute",
            "--packet",
            "/tmp/ripr-8835-missing-packet.json",
            "--result-json",
            "/tmp/ripr-8835-result.json",
            "--json",
            "--cancel-after-ms",
            "0",
            "--cancel-after-ms",
            "1",
        ])?,
        "--cancel-after-ms requires a positive integer; got \"0\"",
    );
    Ok(())
}

#[test]
fn verify_execute_positive_repeated_cancellation_stays_a_policy_refusal()
-> Result<(), Box<dyn std::error::Error>> {
    let result_path =
        std::env::temp_dir().join(format!("ripr-8835-result-{}.json", std::process::id()));
    let result_path = result_path.to_string_lossy().into_owned();
    let output = ripr(&[
        "agent",
        "verify-execute",
        "--root",
        ".",
        "--packet",
        "/tmp/ripr-8835-missing-packet.json",
        "--result-json",
        &result_path,
        "--json",
        "--cancel-after-ms",
        "1",
        "--cancel-after-ms",
        "2",
    ])?;
    assert_eq!(output.status.code(), Some(3));
    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(stdout["disposition"], "verification_rejected_policy");
    assert_eq!(stdout["executed"], false);
    assert_eq!(stdout["result_committed"], false);
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "ripr: verification_rejected_policy\n"
    );
    assert!(!std::path::Path::new(&result_path).exists());
    Ok(())
}
