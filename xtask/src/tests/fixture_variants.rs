//! Fast routing guard for the in-suite fixture variant build (#11197).
//!
//! This exercises the production routing with an in-memory builder and copier;
//! historical real-Cargo runs remain separate integration evidence.
use super::*;
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

#[test]
fn perl_variant_build_never_rewrites_shared_debug_binary() -> Result<(), String> {
    let root = PathBuf::from("guard-target");
    let shared = crate::ripr_debug_binary_in(Some(root.clone()));
    let variant_root =
        std::path::absolute(root.join("fixture-variants")).map_err(|e| e.to_string())?;
    let variant = crate::ripr_debug_binary_in(Some(variant_root.clone()));
    let perl = shared.with_file_name(format!("ripr-lang-perl{}", std::env::consts::EXE_SUFFIX));
    for redirected in [false, true] {
        let files = Mutex::new(BTreeMap::from([(shared.clone(), b"default".to_vec())]));
        let cell = OnceLock::new();
        let result = crate::reports::fixture_variant_binary_built_by(
            shared.clone(),
            &cell,
            &|target| {
                assert_eq!(target, variant_root);
                let output = if redirected {
                    shared.clone()
                } else {
                    variant.clone()
                };
                let mut files = files.lock().map_err(|e| e.to_string())?;
                files.insert(output, b"perl".to_vec());
                // The redirected counterexample also provides the source so copy succeeds.
                files.insert(variant.clone(), b"perl".to_vec());
                Ok(())
            },
            &|source, destination| {
                assert_eq!(source, variant);
                assert_eq!(destination, perl);
                let mut files = files.lock().map_err(|e| e.to_string())?;
                let bytes = files.get(source).ok_or("missing variant")?.clone();
                files.insert(destination.to_path_buf(), bytes);
                Ok(())
            },
        )?;
        assert!(result.ends_with(&perl.to_string_lossy().to_string()));
        let files = files.lock().map_err(|e| e.to_string())?;
        assert_eq!(
            files.get(&perl).map(Vec::as_slice),
            Some(b"perl".as_slice())
        );
        let unchanged = files.get(&shared).map(Vec::as_slice) == Some(b"default".as_slice());
        assert_eq!(
            unchanged, !redirected,
            "redirected builder must trip the guard"
        );
    }
    Ok(())
}
