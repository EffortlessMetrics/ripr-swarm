use super::*;

const CONFIG: &str =
    include_str!("../../tests/data/source-role-corpus/cases/kani_source_context/ripr.toml");

#[test]
fn kani_context_is_explicit_closed_and_path_normalized() -> Result<(), String> {
    assert!(tests_only_parse("")?.languages().rust.kani.is_none());
    let config = tests_only_parse(CONFIG)?;
    let context = config
        .languages()
        .rust
        .kani
        .as_ref()
        .ok_or("missing Kani context")?;
    assert_eq!(context.profile, "kani_ordinary_proof_v1");
    assert_eq!(context.source, Path::new("src/lib.rs"));
    assert!(!context.tests);
    let equivalent = tests_only_parse(&CONFIG.replace("src/lib.rs", " ./src//lib.rs "))?;
    assert_eq!(
        config.languages().rust.kani,
        equivalent.languages().rust.kani
    );
    for invalid in [
        CONFIG.replace("kani_ordinary_proof_v1", "kani_ordinary_proof_v2"),
        CONFIG.replace("src/lib.rs", "../lib.rs"),
        CONFIG.replace("src/lib.rs", "/tmp/lib.rs"),
        CONFIG.replace("src/lib.rs", "src/lib.txt"),
        CONFIG.replace("tests = false", "tests = 'auto'"),
        CONFIG.replace("tests = false", ""),
        format!("{CONFIG}\ncfg = ['host']\n"),
    ] {
        assert!(
            tests_only_parse(&invalid).is_err(),
            "accepted invalid context: {invalid}"
        );
    }
    Ok(())
}

#[test]
fn kani_context_binds_artifact_and_seam_cache_identity() -> Result<(), String> {
    let plain = tests_only_parse("[languages]\nenabled = ['rust']\n")?;
    let configured = tests_only_parse(CONFIG)?;
    for changed in [
        plain,
        tests_only_parse(&CONFIG.replace("tests = false", "tests = true"))?,
        tests_only_parse(&CONFIG.replace("src/lib.rs", "src/main.rs"))?,
    ] {
        assert_ne!(
            check_artifact_config_identity_hash(&configured),
            check_artifact_config_identity_hash(&changed)
        );
        assert_ne!(
            repo_exposure_config_identity_hash(&configured),
            repo_exposure_config_identity_hash(&changed)
        );
    }
    let equivalent = tests_only_parse(&CONFIG.replace("src/lib.rs", "./src/lib.rs"))?;
    assert_eq!(
        check_artifact_config_identity_hash(&configured),
        check_artifact_config_identity_hash(&equivalent)
    );
    assert_eq!(
        repo_exposure_config_identity_hash(&configured),
        repo_exposure_config_identity_hash(&equivalent)
    );
    Ok(())
}
