//! #15284: a CommonJS barrel forwards the related-test link to the owner
//! module the way an ES `export ... from` barrel does. A test that `require()`s
//! `lib/index.js` and calls `qs.parse(...)` runs `lib/parse.js`, so the
//! changed line there is not `no_static_path`. The controls pin the barrels
//! that must stay unlinked: another module's symbol, dynamic requires, a
//! rebound local, a spread overlay, a function owner read as a named member.

use super::*;

/// Whole-function owner (`module.exports = function parse`), line 3 changed.
const FN_OWNER: &str =
    "module.exports = function parse(str) {\n  // parse\n  return str.split('&');\n};\n";
/// Named owner (`exports.parse = function`), line 3 changed.
const NAMED_OWNER: &str =
    "exports.parse = function parse(str) {\n  // parse\n  return str.split('&');\n};\n";
const OTHER: &str = "module.exports = function other(str) {\n  return str;\n};\n";
const QS_TEST: &str = "var qs = require('../lib/index.js');\n\ntest('parses', () => {\n  expect(qs.parse('a=1')).toEqual(['a=1']);\n});\n";

/// The finding's class and whether a related test reaches the owner through
/// the re-export graph (`ReExportChainFollowed`), the relation this change adds.
fn class_in(
    label: &str,
    owner: &str,
    barrel: Option<&str>,
    test_source: &str,
    extra: &[(&str, &str)],
) -> Result<(ExposureClass, bool), String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("lib/parse.js"), owner)?;
    ts_write_file(&root.join("lib/other.js"), OTHER)?;
    if let Some(barrel) = barrel {
        ts_write_file(&root.join("lib/index.js"), barrel)?;
    }
    for (path, contents) in extra {
        ts_write_file(&root.join(path), contents)?;
    }
    ts_write_file(&root.join("test/parse.js"), test_source)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines(
            "lib/parse.js",
            &[(3, "  return str.split('&');")],
        )],
    );
    let _ = std::fs::remove_dir_all(&root);
    let finding = result?
        .findings
        .into_iter()
        .next()
        .ok_or_else(|| format!("{label}: expected a finding"))?;
    let chained = finding.related_tests.iter().any(|test| {
        test.relation_reason == Some(crate::domain::RelationReason::ReExportChainFollowed)
    });
    Ok((finding.class, chained))
}

fn linked(label: &str, owner: &str, barrel: &str, test_source: &str) -> Result<(), String> {
    let (class, chained) = class_in(label, owner, Some(barrel), test_source, &[])?;
    if class == ExposureClass::NoStaticPath || !chained {
        return Err(format!(
            "{label}: barrel did not link the owner through the re-export chain ({class:?}, chained={chained})"
        ));
    }
    Ok(())
}

fn unlinked(label: &str, owner: &str, barrel: &str, test_source: &str) -> Result<(), String> {
    let (class, _) = class_in(label, owner, Some(barrel), test_source, &[])?;
    if class != ExposureClass::NoStaticPath {
        return Err(format!("{label}: expected no_static_path, got {class:?}"));
    }
    Ok(())
}

#[test]
fn object_barrel_with_inline_require_links_a_whole_function_owner() -> Result<(), String> {
    linked(
        "cjs-b-obj",
        FN_OWNER,
        "module.exports = { parse: require('./parse') };\n",
        QS_TEST,
    )
}

#[test]
fn member_pick_barrel_links_a_named_owner() -> Result<(), String> {
    linked(
        "cjs-b-member",
        NAMED_OWNER,
        "exports.parse = require('./parse').parse;\n",
        QS_TEST,
    )?;
    linked(
        "cjs-b-member-mod",
        NAMED_OWNER,
        "module.exports.parse = require('./parse').parse;\n",
        QS_TEST,
    )
}

#[test]
fn whole_module_forward_links_both_owner_shapes() -> Result<(), String> {
    let barrel = "module.exports = require('./parse');\n";
    linked(
        "cjs-b-fwd-fn",
        FN_OWNER,
        barrel,
        "var qs = require('../lib/index.js');\n\ntest('parses', () => {\n  expect(qs('a=1')).toEqual(['a=1']);\n});\n",
    )?;
    linked("cjs-b-fwd-named", NAMED_OWNER, barrel, QS_TEST)
}

#[test]
fn identifier_forwarded_and_shorthand_barrels_link() -> Result<(), String> {
    linked(
        "cjs-b-ident",
        FN_OWNER,
        "var parse = require('./parse');\nmodule.exports = { parse: parse };\n",
        QS_TEST,
    )?;
    linked(
        "cjs-b-short",
        FN_OWNER,
        "const parse = require('./parse');\nmodule.exports = { parse };\n",
        QS_TEST,
    )?;
    linked(
        "cjs-b-ident-exports",
        FN_OWNER,
        "var parse = require('./parse');\nexports.parse = parse;\n",
        QS_TEST,
    )?;
    linked(
        "cjs-b-destructured",
        NAMED_OWNER,
        "const { parse } = require('./parse');\nmodule.exports = { parse };\n",
        QS_TEST,
    )
}

#[test]
fn destructured_require_of_the_barrel_links() -> Result<(), String> {
    linked(
        "cjs-b-destructure-test",
        FN_OWNER,
        "module.exports = { parse: require('./parse') };\n",
        "const { parse } = require('../lib/index.js');\n\ntest('parses', () => {\n  expect(parse('a=1')).toEqual(['a=1']);\n});\n",
    )
}

#[test]
fn renamed_export_links_through_its_exported_name() -> Result<(), String> {
    linked(
        "cjs-b-rename",
        FN_OWNER,
        "module.exports = { decode: require('./parse') };\n",
        "var qs = require('../lib/index.js');\n\ntest('decodes', () => {\n  expect(qs.decode('a=1')).toEqual(['a=1']);\n});\n",
    )
}

#[test]
fn chained_barrels_link_within_the_hop_bound() -> Result<(), String> {
    let (class, chained) = class_in(
        "cjs-b-chain",
        FN_OWNER,
        Some("module.exports = { parse: require('./parse') };\n"),
        "var qs = require('../index.js');\n\ntest('parses', () => {\n  expect(qs.parse('a=1')).toEqual(['a=1']);\n});\n",
        &[("index.js", "module.exports = require('./lib');\n")],
    )?;
    if class == ExposureClass::NoStaticPath || !chained {
        return Err(format!(
            "cjs-b-chain: expected a chained link, got {class:?}"
        ));
    }
    Ok(())
}

#[test]
fn chain_past_the_hop_bound_stays_unlinked() -> Result<(), String> {
    // test -> a0 -> a1 -> a2 -> a3 -> a4 -> lib/parse: six hops.
    let mut extra: Vec<(String, String)> = Vec::new();
    for hop in 0..5 {
        let target = if hop == 4 {
            "./lib/parse".to_string()
        } else {
            format!("./a{}", hop + 1)
        };
        extra.push((
            format!("a{hop}.js"),
            format!("module.exports = require('{target}');\n"),
        ));
    }
    let extra_refs: Vec<(&str, &str)> = extra
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let (class, _) = class_in(
        "cjs-b-deep",
        NAMED_OWNER,
        None,
        "var qs = require('../a0.js');\n\ntest('parses', () => {\n  expect(qs.parse('a=1')).toEqual(['a=1']);\n});\n",
        &extra_refs,
    )?;
    if class != ExposureClass::NoStaticPath {
        return Err(format!(
            "cjs-b-deep: expected no_static_path, got {class:?}"
        ));
    }
    Ok(())
}

#[test]
fn barrel_exporting_a_different_module_does_not_link_the_owner() -> Result<(), String> {
    unlinked(
        "cjs-c-other",
        FN_OWNER,
        "module.exports = { parse: require('./other') };\n",
        QS_TEST,
    )?;
    unlinked(
        "cjs-c-other-fwd",
        NAMED_OWNER,
        "module.exports = require('./other');\n",
        QS_TEST,
    )?;
    unlinked(
        "cjs-c-other-member",
        NAMED_OWNER,
        "exports.parse = require('./other').parse;\n",
        QS_TEST,
    )?;
    // The owner is exported only as `stringify`; `qs.parse` is `other`.
    unlinked(
        "cjs-c-other-name",
        FN_OWNER,
        "module.exports = { parse: require('./other'), stringify: require('./parse') };\n",
        QS_TEST,
    )
}

#[test]
fn named_member_of_a_whole_function_module_is_not_the_owner() -> Result<(), String> {
    // `require('./parse').parse` is a property of the function: undefined.
    unlinked(
        "cjs-c-fn-member",
        FN_OWNER,
        "exports.parse = require('./parse').parse;\n",
        QS_TEST,
    )
}

#[test]
fn dynamic_or_computed_requires_stay_unlinked() -> Result<(), String> {
    unlinked(
        "cjs-c-dyn",
        FN_OWNER,
        "var name = './parse';\nmodule.exports = { parse: require(name) };\n",
        QS_TEST,
    )?;
    unlinked(
        "cjs-c-concat",
        FN_OWNER,
        "var n = 'parse';\nmodule.exports = { parse: require('./' + n) };\n",
        QS_TEST,
    )?;
    unlinked(
        "cjs-c-computed-key",
        FN_OWNER,
        "var k = 'parse';\nmodule.exports = { [k]: require('./parse') };\n",
        QS_TEST,
    )?;
    unlinked(
        "cjs-c-computed-assign",
        FN_OWNER,
        "var k = 'parse';\nexports[k] = require('./parse');\n",
        QS_TEST,
    )
}

#[test]
fn rebound_overlaid_or_spread_forwards_stay_unlinked() -> Result<(), String> {
    unlinked(
        "cjs-c-rebound",
        FN_OWNER,
        "var parse = require('./parse');\nparse = require('./other');\nmodule.exports = { parse: parse };\n",
        QS_TEST,
    )?;
    unlinked(
        "cjs-c-spread",
        FN_OWNER,
        "var more = require('./other');\nmodule.exports = { parse: require('./parse'), ...more };\n",
        QS_TEST,
    )?;
    unlinked(
        "cjs-c-twice",
        FN_OWNER,
        "exports.parse = require('./parse');\nexports.parse = require('./other');\n",
        QS_TEST,
    )?;
    unlinked(
        "cjs-c-overlay",
        NAMED_OWNER,
        "module.exports = require('./parse');\nmodule.exports.parse = function () {};\n",
        QS_TEST,
    )?;
    unlinked(
        "cjs-c-module-use",
        FN_OWNER,
        "module.exports = { parse: require('./parse') };\nif (cond) { module.exports.parse = null; }\n",
        QS_TEST,
    )
}

#[test]
fn a_mocked_barrel_source_breaks_the_chain() -> Result<(), String> {
    unlinked(
        "cjs-c-mocked",
        FN_OWNER,
        "module.exports = { parse: require('./parse') };\n",
        "var qs = require('../lib/index.js');\njest.mock('../lib/index.js');\n\ntest('parses', () => {\n  expect(qs.parse('a=1')).toEqual(['a=1']);\n});\n",
    )
}

#[test]
fn a_shadowed_or_rewritten_namespace_stays_unlinked() -> Result<(), String> {
    unlinked(
        "cjs-c-shadow",
        FN_OWNER,
        "module.exports = { parse: require('./parse') };\n",
        "var qs = require('../lib/index.js');\n\ntest('parses', () => {\n  var qs = { parse: (s) => s };\n  expect(qs.parse('a=1')).toEqual('a=1');\n});\n",
    )
}

#[test]
fn barrel_extraction_records_only_literal_relative_requires() {
    let facts = |source: &str| {
        let allocator = oxc_allocator::Allocator::default();
        let parsed =
            oxc_parser::Parser::new(&allocator, source, oxc_span::SourceType::cjs()).parse();
        super::super::commonjs_reexports::commonjs_reexports(
            Path::new("lib/index.js"),
            &parsed.program.body,
            None,
            Path::new("."),
        )
    };
    let object = facts("module.exports = { parse: require('./parse'), n: require('./a').m };\n");
    let mut entries = object.entries.clone();
    entries.sort();
    assert_eq!(
        entries,
        vec![
            (
                ("lib/index".to_string(), "n".to_string()),
                ("m".to_string(), "lib/a".to_string())
            ),
            (
                ("lib/index".to_string(), "parse".to_string()),
                ("default".to_string(), "lib/parse".to_string())
            ),
        ]
    );
    assert!(object.forwards.is_empty());
    let forward = facts("module.exports = require('./parse');\n");
    assert_eq!(
        forward.forwards,
        vec![("lib/index".to_string(), "lib/parse".to_string())]
    );
    // A package specifier is not an in-repo module.
    assert!(
        facts("module.exports = { x: require('lodash') };\n")
            .entries
            .is_empty()
    );
}

/// A whole-module forward exposes only what the source exports. A
/// declaration the source never assigns onto `exports` is `undefined` on the
/// barrel, so `qs.helper(...)` cannot reach it (review of #15284).
#[test]
fn whole_forward_does_not_reach_an_unexported_declaration() -> Result<(), String> {
    let owner = "exports.parse = function parse(s) { return helper(s); };\nfunction helper(str) {\n  return str.split('&');\n}\n";
    let test_source = "var qs = require('../lib/index.js');\n\ntest('helper', () => {\n  expect(qs.helper('a=1')).toEqual(['a=1']);\n});\n";
    unlinked(
        "cjs-r-unexported",
        owner,
        "module.exports = require('./parse');\n",
        test_source,
    )
}

/// The same name written twice over a whole forward: the last value wins at
/// runtime, so neither write nor the forward links the owner.
#[test]
fn a_twice_overlaid_name_over_a_forward_stays_unlinked() -> Result<(), String> {
    unlinked(
        "cjs-r-dup-overlay",
        NAMED_OWNER,
        "module.exports = require('./parse');\nmodule.exports.parse = require('./other');\nmodule.exports.parse = require('./other');\n",
        QS_TEST,
    )
}

/// Exports rewritten from a nested scope, through a rebound `exports`, a
/// local `require`, or a top-level `this`, refuse the whole barrel.
#[test]
fn hidden_export_rewrites_refuse_the_barrel() -> Result<(), String> {
    for (label, barrel) in [
        (
            "cjs-r-nested-fn",
            "exports.parse = require('./parse').parse;\nfunction init() { module.exports.parse = require('./other'); }\ninit();\n",
        ),
        (
            "cjs-r-try",
            "exports.parse = require('./parse').parse;\ntry { exports.parse = require('./other'); } catch (e) {}\n",
        ),
        (
            "cjs-r-rebind",
            "exports.parse = require('./parse').parse;\nexports = { parse: require('./other') };\n",
        ),
        (
            "cjs-r-local-require",
            "function require(p) { return { parse: function () {} }; }\nexports.parse = require('./parse').parse;\n",
        ),
        (
            "cjs-r-this",
            "exports.parse = require('./parse').parse;\nthis.parse = require('./other');\n",
        ),
    ] {
        unlinked(label, NAMED_OWNER, barrel, QS_TEST)?;
    }
    Ok(())
}

/// A source module mocked behind an unmocked barrel breaks the chain, and a
/// `qs.parse(` that appears only in a comment or string, or on a different
/// local, does not link.
#[test]
fn a_mocked_source_or_a_non_call_mention_stays_unlinked() -> Result<(), String> {
    let barrel = "module.exports = { parse: require('./parse') };\n";
    unlinked(
        "cjs-r-mock-source",
        FN_OWNER,
        barrel,
        "jest.mock('../lib/parse');\nvar qs = require('../lib/index.js');\n\ntest('parses', () => {\n  expect(qs.parse('a=1')).toEqual(['a=1']);\n});\n",
    )?;
    unlinked(
        "cjs-r-comment",
        FN_OWNER,
        barrel,
        "var qs = require('../lib/index.js');\n\ntest('parses', () => {\n  // qs.parse('a=1')\n  expect('qs.parse(1)').toEqual('x');\n});\n",
    )?;
    unlinked(
        "cjs-r-other-local",
        FN_OWNER,
        barrel,
        "var qs = require('../lib/index.js');\nvar other = require('../lib/other.js');\n\ntest('parses', () => {\n  expect(other.parse('a=1')).toEqual(['a=1']);\n});\n",
    )
}

/// `this` inside a function or class body is not `module.exports`, so it
/// leaves the barrel readable; a top-level arrow inherits module `this`, so
/// a write there refuses the barrel.
#[test]
fn only_module_scope_this_refuses_the_barrel() -> Result<(), String> {
    linked(
        "cjs-b-this-fn",
        NAMED_OWNER,
        "exports.parse = require('./parse').parse;\nfunction F() { this.parse = 1; }\nclass C { m() { this.parse = 1; } }\n",
        QS_TEST,
    )?;
    unlinked(
        "cjs-r-this-arrow",
        NAMED_OWNER,
        "exports.parse = require('./parse').parse;\nconst f = () => { this.parse = require('./other'); };\nf();\n",
        QS_TEST,
    )
}

/// A whole forward into a source that exports through a shorthand object
/// literal reaches the named owner.
#[test]
fn whole_forward_reaches_a_shorthand_object_export() -> Result<(), String> {
    linked(
        "cjs-b-fwd-short",
        "function parse(str) {\n  // parse\n  return str.split('&');\n}\nmodule.exports = { parse };\n",
        "module.exports = require('./parse');\n",
        QS_TEST,
    )
}

/// `require()` of an ES module is its namespace object, so calling the
/// forwarded whole value is not a call of the ES default export.
#[test]
fn whole_forward_of_an_es_default_is_not_the_owner() -> Result<(), String> {
    unlinked(
        "cjs-r-es-default",
        "export default function parse(str) {\n  // parse\n  return str.split('&');\n}\n",
        "module.exports = require('./parse');\n",
        "var qs = require('../lib/index.js');\n\ntest('parses', () => {\n  expect(qs('a=1')).toEqual(['a=1']);\n});\n",
    )
}
