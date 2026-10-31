//! Fast production-entry routing guard (#11197). Historical real Cargo builds
//! remain separate integration evidence; this test performs no build or copy.
use super::*;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::OnceLock;

#[test]
fn perl_variant_build_never_rewrites_shared_debug_binary() -> Result<(), String> {
    use crate::reports::{ripr_perl_fixture_binary, variant_test};
    let root = std::env::current_dir()
        .map_err(|e| e.to_string())?
        .join("guard-target");
    let shared = crate::ripr_debug_binary_in(Some(root.clone()));
    let variant_root =
        std::path::absolute(root.join("fixture-variants")).map_err(|e| e.to_string())?;
    let perl = shared.with_file_name(format!("ripr-lang-perl{}", std::env::consts::EXE_SUFFIX));
    let context = Rc::new(variant_test::Context {
        shared: shared.clone(),
        once: OnceLock::new(),
        files: RefCell::new(BTreeMap::from([(shared.clone(), b"default".to_vec())])),
        commands: RefCell::new(Vec::new()),
    });
    let result = variant_test::with_context(context.clone(), || {
        std::thread::spawn(|| assert!(variant_test::active().is_none()))
            .join()
            .map_err(|_panic| "thread isolation check panicked")?;
        ripr_perl_fixture_binary()
    })?;
    assert!(
        variant_test::active().is_none(),
        "override must be restored"
    );
    assert!(result.ends_with(&perl.to_string_lossy().to_string()));
    let commands = context.commands.borrow();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].0, "cargo");
    assert_eq!(
        commands[0].1,
        ["build", "-p", "ripr", "--features", "lang-perl"]
    );
    assert_eq!(commands[0].2.as_ref(), Some(&variant_root));
    let files = context.files.borrow();
    assert_eq!(
        files.get(&shared).map(Vec::as_slice),
        Some(b"default".as_slice())
    );
    assert_eq!(
        files.get(&perl).map(Vec::as_slice),
        Some(b"perl".as_slice())
    );
    Ok(())
}

#[test]
fn variant_override_is_restored_after_unwind() {
    use crate::reports::variant_test;
    let context = Rc::new(variant_test::Context {
        shared: PathBuf::from("unused"),
        once: OnceLock::new(),
        files: RefCell::new(BTreeMap::new()),
        commands: RefCell::new(Vec::new()),
    });
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        variant_test::with_context(context, || std::panic::resume_unwind(Box::new("cleanup")));
    }));
    assert!(result.is_err());
    assert!(variant_test::active().is_none());
}
