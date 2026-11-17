//! Multi-site reach (#14918): one caller that calls the helper at several
//! direct sites carries reach to it, but binds no rows and forwards nothing.

use super::*;
use crate::analysis::facts::OwnedRustIndex;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn index_of(source: &str) -> Result<RustIndex, String> {
    let facts =
        crate::analysis::syntax::ra::summarize_file_with_parser(Path::new("src/lib.rs"), source)?;
    let mut by_file = BTreeMap::new();
    by_file.insert(PathBuf::from("src/lib.rs"), facts.clone());
    Ok(RustIndex::from_owned(OwnedRustIndex {
        functions: facts.functions.clone(),
        tests: facts.tests.clone(),
        files: by_file,
        ..Default::default()
    }))
}

const NUMBER: &str = "fn number(s: &str) -> Result<u64, ()> {\n    if s.len() > 1 && s.starts_with('0') {\n        return Err(());\n    }\n    s.parse().map_err(|_| ())\n}\n";

fn chain_with(entry: &str) -> Result<HelperChain, String> {
    let source = format!("{NUMBER}{entry}");
    Ok(resolve_chain("number", &index_of(&source)?, true, &[]))
}

#[test]
fn several_sites_in_one_caller_carry_reach_only() -> Result<(), String> {
    // Separate lines (the #14918 `parse` shape) and one line holding both
    // calls are each several sites.
    for entry in [
        "pub fn parse(a: &str, b: &str) -> Result<(u64, u64), ()> {\n    Ok((\n        number(a)?,\n        number(b)?,\n    ))\n}\n",
        "pub fn parse(a: &str, b: &str) -> Result<(u64, u64), ()> {\n    Ok((number(a)?, number(b)?))\n}\n",
    ] {
        let chain = chain_with(entry)?;
        assert_eq!(chain.entry_name(), Some("parse"), "{chain:?}");
        assert!(
            chain
                .stop_above
                .as_deref()
                .is_none_or(|edge| !edge.contains("multi-site")),
            "{chain:?}"
        );
        let [hop] = chain.hops.as_slice() else {
            return Err(format!("one hop expected: {chain:?}"));
        };
        assert!(hop.multi_site && hop.reach_only() && !hop.through_macro);
        // No site's arguments stand for the others: nothing binds.
        assert!(hop.arguments.is_empty(), "{hop:?}");
        assert!(!chain_forwards_owner_result("number", &chain));
    }
    Ok(())
}

#[test]
fn direct_call_count_reads_whole_word_calls() {
    assert_eq!(
        direct_call_count("Ok((number(a)?, number(b)?))", "number"),
        2
    );
    assert_eq!(direct_call_count("number (a)", "number"), 1);
    assert_eq!(direct_call_count("renumber(a) + number_of(b)", "number"), 0);
    assert_eq!(direct_call_count("let number = 3;", "number"), 0);
}

#[test]
fn a_single_site_still_binds_its_arguments() -> Result<(), String> {
    let chain = chain_with("pub fn parse(a: &str) -> Result<u64, ()> {\n    number(a)\n}\n")?;
    let [hop] = chain.hops.as_slice() else {
        return Err(format!("one hop expected: {chain:?}"));
    };
    assert!(!hop.multi_site && !hop.reach_only());
    assert_eq!(hop.arguments, vec!["a".to_string()]);
    assert!(chain_forwards_owner_result("number", &chain));
    Ok(())
}

#[test]
fn multi_site_keeps_every_other_stop_edge() -> Result<(), String> {
    // Two callers that both call the helper twice: the multi-caller rule
    // still decides, and with no test reaching either, nothing resolves.
    let chain = chain_with(
        "pub fn first(a: &str) -> bool {\n    number(a).is_ok() && number(a).is_ok()\n}\npub fn second(a: &str) -> bool {\n    number(a).is_ok() || number(a).is_ok()\n}\n",
    )?;
    assert!(chain.hops.is_empty(), "{chain:?}");
    assert!(chain.stop_above.is_some(), "{chain:?}");
    // A duplicate helper name stays ambiguous however many sites call it.
    let source = format!(
        "{NUMBER}mod other {{\n    pub fn number(s: &str) -> Result<u64, ()> {{ s.parse().map_err(|_| ()) }}\n}}\npub fn parse(a: &str, b: &str) -> bool {{\n    number(a).is_ok() && number(b).is_ok()\n}}\n"
    );
    let chain = resolve_chain("number", &index_of(&source)?, true, &[]);
    assert!(chain.hops.is_empty(), "{chain:?}");
    Ok(())
}

#[test]
fn a_test_side_caller_keeps_the_multi_site_stop() -> Result<(), String> {
    // A test that calls the owner twice relates to it directly; it is never
    // a reach-only hop.
    let chain = chain_with(
        "#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn two_calls() {\n        assert!(number(\"1\").is_ok());\n        assert!(number(\"01\").is_err());\n    }\n}\n",
    )?;
    assert!(chain.hops.iter().all(|hop| !hop.multi_site), "{chain:?}");
    Ok(())
}
