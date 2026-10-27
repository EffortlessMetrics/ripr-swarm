//! #10807: claims' `assert_ok!(call)` / `assert_some!(call)` yield the `Ok` /
//! `Some` payload and panic on the other variant, so the payload route may
//! credit only a pin on that same payload. Beside an early `Err(..)` change
//! the pin is on the opposite variant; the route must not read it, whatever
//! the import. End to end this is invisible (the general direct-owner-call
//! path reads `exposed` on its own), so the gate is pinned here.

use super::*;
use crate::analysis::facts::drop_in::{DropInManifests, temp_workspace};

const OK_LIB: &str = "pub fn checked_div(a: u32, b: u32) -> Result<u32, ()> {\n    if b == 0 {\n        return Err(());\n    }\n    Ok(a / b)\n}\n";

fn verified_claims_root() -> Result<PathBuf, String> {
    temp_workspace(
        "claims-direction",
        &[(
            "Cargo.toml",
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[dev-dependencies]\nclaims = \"0.8\"\n",
        )],
    )
}

/// The assertions admitted for the one test in `tests`, on the pin of
/// `lib`'s `owner` at `expression` (a probe of `family`) under `root`.
fn admitted_under(
    root: &Path,
    lib: &str,
    owner_name: &str,
    family: ProbeFamily,
    expression: &str,
    tests: &str,
) -> Result<Vec<String>, String> {
    let mut index = index(&[(LIB, lib), (TESTS, tests)]);
    index.drop_in_manifests = DropInManifests::new(root);
    let owner = owner(&index, owner_name);
    let line = owner.start_line
        + owner
            .body
            .lines()
            .position(|line| line.contains(expression.trim_end_matches(';')))
            .ok_or("changed line")?;
    let probe = Probe {
        family,
        location: SourceLocation::new(owner.file.clone(), line, 1),
        ..return_probe(owner, expression)
    };
    let pin = OwnerReturnPin::establish(&probe, owner, &index).ok_or("the pin must establish")?;
    Ok(admitted_texts(&index, &pin))
}

#[test]
fn the_claims_payload_route_reads_only_the_variant_it_yields() -> Result<(), String> {
    let root = verified_claims_root()?;
    let ok_body = "use demo::checked_div;\nuse claims::assert_ok;\n\n#[test]\nfn divides() {\n    let q = assert_ok!(checked_div(17, 5));\n    assert_eq!(q, 3);\n}\n";
    // Control: the import verifies and the `Ok` tail pin credits through the
    // route, so the refusal below is the direction gate, not the import.
    assert_eq!(
        admitted_under(
            &root,
            OK_LIB,
            "checked_div",
            ProbeFamily::ReturnValue,
            "Ok(a / b)",
            ok_body
        )?,
        vec!["assert_eq!(q, 3);".to_string()]
    );
    // The same route beside the early `Err(..)` change: `assert_ok!` panics
    // on the `Err` the changed exit builds, so its payload says nothing of it.
    let err_lib = format!("{MISSING_EQUALS_LIB}\n");
    let err_body = "use demo::{parse_pairs, ConfigError};\nuse claims::assert_ok;\n\n#[test]\nfn reads() {\n    let q = assert_ok!(parse_pairs(\"port\"));\n    assert_eq!(q, ConfigError::MissingEquals { line: 1 });\n}\n";
    assert_eq!(
        admitted_under(
            &root,
            &err_lib,
            "parse_pairs",
            ProbeFamily::ErrorPath,
            "return Err(ConfigError::MissingEquals { line: idx + 1 });",
            err_body
        )?,
        Vec::<String>::new()
    );
    Ok(())
}
