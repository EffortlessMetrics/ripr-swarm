//! #13703: a per-name binding-site query skips a file whose union scan found
//! no site. These tests compare that reading with the one that rescans.

use super::*;

const TRUSTED: &[&str] = &[
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "format",
    "json",
    "matches",
    "panic",
    "unreachable",
    "vec",
];

struct Rescan;

impl Rescan {
    fn on() -> Self {
        RESCAN_SITELESS_FILES.with(|rescan| rescan.set(true));
        Self
    }
}

impl Drop for Rescan {
    fn drop(&mut self) {
        RESCAN_SITELESS_FILES.with(|rescan| rescan.set(false));
    }
}

fn scans() -> usize {
    BINDING_SITE_SCANS.with(std::cell::Cell::get)
}

fn site_sets(source: &str) -> (NamedMacroSites, NamedMacroSites) {
    trusted_macro_binding_site_sets(
        source,
        &BTreeSet::from(["workspace_member".to_string()]),
        TRUSTED,
        TRUSTED,
        &|_, _| false,
        &|_| false,
    )
}

/// Both readings of `source`, and how many scans the skipping one ran.
fn both_readings(source: &str) -> (NamedMacroSites, NamedMacroSites, usize) {
    let before = scans();
    let skipping = site_sets(source);
    let skipping_scans = scans() - before;
    let rescanning = {
        let _rescan = Rescan::on();
        site_sets(source)
    };
    assert_eq!(skipping, rescanning, "{source}");
    (skipping.0, skipping.1, skipping_scans)
}

#[test]
fn a_siteless_file_is_scanned_once_and_reads_as_the_rescan_does() {
    let mut siteless = String::from("use std::collections::BTreeMap;\n");
    for n in 0..30 {
        siteless.push_str(&format!(
            "#[test]\nfn t{n}() {{ let v = vec![{n}]; assert_eq!(v.len(), 1); assert!(matches!(v[0], {n})); }}\n"
        ));
    }
    let (union, singletons, skipping_scans) = both_readings(&siteless);
    assert!(union.is_empty(), "premise: no site in {union:?}");
    assert!(singletons.is_empty(), "{singletons:?}");
    assert_eq!(
        skipping_scans, 1,
        "only the union scan reads a siteless file"
    );

    let before = scans();
    let _rescan = Rescan::on();
    let _ = site_sets(&siteless);
    assert_eq!(
        scans() - before,
        1 + TRUSTED.len(),
        "premise: the rescanning reading asks once per name"
    );
}

#[test]
fn files_with_sites_keep_every_per_name_scan() {
    // Each source binds or may bind a trusted name a different way; the
    // per-name queries must still run and agree with the rescanning reading.
    let sources = [
        "macro_rules! assert_eq { ($($t:tt)*) => {}; }\nfn f() { assert_eq!(1, 1); }\n",
        "use other::*;\nfn f() { assert!(true); }\n",
        "#[macro_use]\nextern crate other;\nfn f() { assert!(true); }\n",
        "use other::assert_eq;\nfn f() { assert_eq!(1, 1); }\n",
        "fn f() { define!(assert_eq!(mod tests;)); }\n",
        "macro_rules! vec { ($($t:tt)*) => {}; }\nfn f() { vec!(assert_eq!(mod tests;)); assert!(assert_eq!(1, 1)); }\n",
        "macro_rules! make { () => { macro_rules! assert_ne { () => {}; } }; }\n",
        "macro_rules! make { () => { use other::*; }; }\n",
        "#![no_implicit_prelude]\nfn f() {}\n",
        "extern crate other as std;\n",
        "fn f( { assert!(true)\n",
    ];
    let mut with_union = 0;
    let mut with_singletons = 0;
    for source in sources {
        let (union, singletons, skipping_scans) = both_readings(source);
        assert!(!union.is_empty(), "premise: a site in {source}");
        assert!(
            singletons
                .iter()
                .all(|(name, _)| TRUSTED.contains(&name.as_str())),
            "{singletons:?}"
        );
        with_union += 1;
        if !singletons.is_empty() {
            with_singletons += 1;
            // The unparsed file returns before any scan.
            assert!(
                skipping_scans == 0 || skipping_scans == 1 + TRUSTED.len(),
                "{skipping_scans} scans of {source}"
            );
        }
    }
    assert_eq!(with_union, sources.len());
    assert!(
        with_singletons >= 8,
        "{with_singletons} sources report per-name sites"
    );
}

/// The claim behind the skip, on sources this repository ships: wherever the
/// union scan finds nothing, no per-name query finds anything either.
#[test]
fn this_crates_syntax_sources_read_the_same_either_way() -> Result<(), String> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/analysis/syntax");
    let mut files = Vec::new();
    let mut pending = vec![dir];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).map_err(|error| error.to_string())? {
            let path = entry.map_err(|error| error.to_string())?.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut siteless = 0;
    let mut read = 0;
    for path in &files {
        let source = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
        // Debug-build scans of the largest files take minutes; the claim is
        // per file, so the bounded ones are the corpus.
        if source.len() > 96 * 1024 {
            continue;
        }
        let (union, singletons, _) = both_readings(&source);
        read += 1;
        if union.is_empty() {
            siteless += 1;
            assert!(singletons.is_empty(), "{}", path.display());
        }
    }
    assert!(read >= 10, "premise: {read} sources read");
    assert!(siteless >= 5, "premise: {siteless} siteless sources");
    Ok(())
}
