//! Structural production-entry routing guard (#11197). The simulated writes
//! do not establish what a real Cargo build or filesystem does.
use super::*;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::OnceLock;

#[test]
fn perl_variant_routing_simulation_records_no_shared_write() -> Result<(), String> {
    use crate::reports::{ripr_perl_fixture_binary, variant_test};
    // Identity only: nothing is created under this path, and it must not depend
    // on the process working directory, which other tests change.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("guard-target");
    let shared = crate::ripr_debug_binary_in(Some(root.clone()));
    let variant_root =
        std::path::absolute(root.join("fixture-variants")).map_err(|e| e.to_string())?;
    let perl = shared.with_file_name(format!("ripr-lang-perl{}", std::env::consts::EXE_SUFFIX));
    let context = Rc::new(variant_test::Context {
        shared: shared.clone(),
        once: OnceLock::new(),
        files: RefCell::new(BTreeMap::from([(shared.clone(), b"default".to_vec())])),
        commands: RefCell::new(Vec::new()),
        writes: RefCell::new(Vec::new()),
    });
    let initial_generation = context.shared_generation();
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
    assert_eq!(context.shared_generation(), initial_generation);
    let files = context.files.borrow();
    assert_eq!(
        files.get(&shared).map(Vec::as_slice),
        Some(b"default".as_slice())
    );
    assert_eq!(
        files.get(&perl).map(Vec::as_slice),
        Some(b"perl".as_slice())
    );
    drop(files);
    // The write oracle must detect even a rewrite that leaves identical bytes.
    context.write(shared.clone(), b"default".to_vec());
    assert_eq!(
        context.files.borrow().get(&shared),
        Some(&b"default".to_vec())
    );
    assert_ne!(context.shared_generation(), initial_generation);
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
        writes: RefCell::new(Vec::new()),
    });
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        variant_test::with_context(context, || std::panic::resume_unwind(Box::new("cleanup")));
    }));
    assert!(result.is_err());
    assert!(variant_test::active().is_none());
}
