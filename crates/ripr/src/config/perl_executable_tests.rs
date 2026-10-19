use super::parse_config;
use std::path::Path;

#[test]
fn perl_executable_rejects_an_empty_path() {
    let result = parse_config("[perl]\nexecutable = ''\n");
    assert!(
        matches!(result, Err(ref message) if message == "perl.executable must not be empty; remove the key to use the default exporter"),
        "an empty executable must fail config validation: {result:?}"
    );
}

#[test]
fn perl_executable_rejects_nul_without_echoing_the_path() {
    for value in [r#""\u0000""#, r#""tools/\u0000exporter""#] {
        let result = parse_config(&format!("[perl]\nexecutable = {value}\n"));
        assert!(
            matches!(result, Err(ref message) if message == "perl.executable must not contain NUL characters"),
            "a NUL-bearing executable must fail with a source-free diagnostic: {result:?}"
        );
    }
}

#[test]
fn perl_executable_preserves_valid_paths_and_the_opt_in_boundary() -> Result<(), String> {
    for value in [
        "perl-ripr-facts",
        "./tools/exporter",
        "/opt/Perl Tools/exporter",
        r"C:\Program Files\Perl\exporter.exe",
        "./tools/导出器",
        " ./tools/exporter ",
    ] {
        let config = parse_config(&format!("[perl]\nexecutable = '{value}'\n"))?;
        assert_eq!(config.perl().executable_for_opt_in(false), None);
        assert_eq!(
            config.perl().executable_for_opt_in(true).and_then(Path::to_str),
            Some(value),
            "valid executable paths must keep their exact spelling"
        );
    }
    assert!(parse_config("[perl]\n")?.perl().executable.is_none());
    Ok(())
}
