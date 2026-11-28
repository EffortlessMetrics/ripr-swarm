use super::*;

#[test]
fn associated_glob_memo_reuses_full_decisions_without_changing_refusals() -> Result<(), String> {
    let root = crate::analysis::facts::drop_in::temp_workspace(
        "associated-glob-memo",
        &[
            (
                "Cargo.toml",
                "[workspace]\nmembers=['meter','spare','panel']\nresolver='2'\n",
            ),
            (
                "meter/Cargo.toml",
                "[package]\nname='meter'\nversion='0.1.0'\n",
            ),
            (
                "spare/Cargo.toml",
                "[package]\nname='spare'\nversion='0.1.0'\n",
            ),
            (
                "panel/Cargo.toml",
                "[package]\nname='panel'\nversion='0.1.0'\n[dependencies]\nmeter={path='../meter'}\nspare={path='../spare'}\n",
            ),
            (
                "meter/src/lib.rs",
                "pub struct Lamp;\nimpl Lamp {\n    pub fn lumens() -> u32 {\n        8 * 100\n    }\n}\n",
            ),
            ("spare/src/lib.rs", "pub fn lumens() -> u32 { 0 }\n"),
            ("panel/src/lib.rs", ""),
            (
                "panel/tests/globbed.rs",
                "use meter::*;\n#[test]\nfn lamp() {\n    assert_eq!(Lamp::lumens(), 800);\n    assert_eq!(Lamp::lumens(), 800);\n}\n",
            ),
        ],
    )?;
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let cleanup = Cleanup(root);
    let files = [
        "meter/src/lib.rs",
        "spare/src/lib.rs",
        "panel/src/lib.rs",
        "panel/tests/globbed.rs",
    ]
    .map(PathBuf::from);
    let index = crate::analysis::facts::build_index(&cleanup.0, &files)?;
    let owner = index
        .functions()
        .iter()
        .find(|owner| owner.name == "lumens" && owner.file == Path::new("meter/src/lib.rs"))
        .ok_or("owner missing")?;
    let test = index
        .tests()
        .iter()
        .find(|test| test.name == "lamp")
        .ok_or("test missing")?;
    let facts = index.files().get(&test.file).ok_or("test source missing")?;
    let source = facts.source.as_ref();
    let binding = FreeOwnerBinding {
        name: &owner.name,
        owner_file: &owner.file,
        owner_start_line: owner.start_line,
    };
    let syntax = OwnerPinSyntax::default();
    let pin = OwnerReturnPin::establish(&return_probe(owner, "8 * 100"), owner, &index)
        .ok_or("pin missing")?;
    assert_eq!(test.assertions.len(), 2);
    for _ in 0..32 {
        for assertion in &test.assertions {
            assert!(pin.admits(test, assertion, &index, &|_, _| false, &syntax));
        }
    }
    assert_eq!(
        *syntax.associated_glob_computations.borrow(),
        1,
        "64 real admissions must compute this whole-index root decision once"
    );
    for _ in 0..32 {
        assert!(!associated_glob_root_is_unrebound(
            &binding, "spare", test, source, &index, &syntax
        ));
    }
    assert_eq!(
        *syntax.associated_glob_computations.borrow(),
        2,
        "a refusal is also memoized under its distinct root"
    );
    let stale = format!("{source}\n");
    assert!(!associated_glob_root_is_unrebound(
        &binding, "meter", test, &stale, &index, &syntax
    ));
    assert_eq!(
        *syntax.associated_glob_computations.borrow(),
        2,
        "stale source must not borrow or poison a captured-file entry"
    );
    assert!(associated_glob_root_is_unrebound(
        &binding, "meter", test, source, &index, &syntax
    ));
    // Executed uncached control: same decision, repeated full work. A wrapper
    // that bypasses the memo would fail the literal one-computation pin above.
    let uncached = OwnerPinSyntax::default();
    for _ in 0..8 {
        assert!(associated_glob_root_compute(
            &binding, "meter", test, source, &index, &uncached
        ));
    }
    assert_eq!(*uncached.associated_glob_computations.borrow(), 8);
    Ok(())
}
