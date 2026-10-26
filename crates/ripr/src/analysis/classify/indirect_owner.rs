//! RIPR-SPEC-0240: an owner a test reaches without a call ripr can place is
//! not a gap.
//!
//! Three source forms leave a test whose oracle is real and whose reading is
//! `weakly_exposed`, a certain "assertion too weak" claim, because ripr sees no
//! `owner(..)` call it can pair with the assertion:
//!
//! - **function value**: the test names the owner without calling it
//!   (`.map(owner)`, an untyped `let f = owner; f(2)`), so the call
//!   happens inside machinery ripr does not follow; a typed
//!   `let f: fn(..) -> .. = owner;` binding is left to the typed
//!   function-pointer reading (RIPR-SPEC-0115, #9478);
//! - **generated owner**: the changed line is an item-position macro
//!   invocation (`def_add!(add_five, 2 + 3);`) that defines the owner, and the
//!   expansion is not source ripr reads;
//! - **executor-wrapped call**: the owner call is the only argument of
//!   `block_on(..)`, a local helper polling a `Future` or a known executor,
//!   and the assertion reads the helper's result.
//!
//! None of these is credited; the gap is withheld under
//! `rust_indirect_owner_call_unresolved`. Like every RIPR-SPEC-0240 rule this
//! only withholds, so the actionability flip stays fail-closed.

use super::gap_admission::{is_ignored, withhold_gap};
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::facts::{FunctionContainer, FunctionFact, TestFact};
use crate::analysis::rust_index::{RustIndex, is_test_file};
use crate::domain::*;

const LIMIT: StaticLimitKind = StaticLimitKind::RustIndirectOwnerCallUnresolved;
const FIRST_UNRESOLVED_EDGE: &str =
    "whether the assertion observes the owner reached without a call ripr can place";

/// Callee names read as running a future to completion even when their
/// definition is not in the workspace (`futures::executor::block_on`,
/// `pollster::block_on`, a runtime's `rt.block_on(..)`).
const KNOWN_EXECUTORS: [&str; 1] = ["block_on"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
    FunctionValue,
    Generated,
    ExecutorWrapped,
}

impl Form {
    fn advice(self) -> &'static str {
        match self {
            Form::FunctionValue => {
                "passes the owner as a function value, and ripr does not follow the indirect call to the assertion"
            }
            Form::Generated => {
                "reaches an owner a macro generates, and ripr does not expand the macro"
            }
            Form::ExecutorWrapped => {
                "calls the owner inside an executor helper such as `block_on`, and ripr does not pair the assertion on the helper's result with the owner's call"
            }
        }
    }
}

/// Withhold a gap whose reaching tests use one of the forms above. Returns
/// `true` when the finding was rewritten to `static_unknown`.
pub(in crate::analysis) fn withhold_indirect_owner_gap(
    finding: &mut Finding,
    index: &RustIndex,
) -> bool {
    if finding.static_limit_kind.is_some()
        || !matches!(
            finding.class,
            ExposureClass::WeaklyExposed | ExposureClass::ReachableUnrevealed
        )
    {
        return false;
    }
    let changed_line = finding.probe.location.line;
    let owner = finding.probe.owner.as_ref().and_then(|owner_id| {
        index.functions().iter().find(|function| {
            function.id == *owner_id
                && function.start_line <= changed_line
                && changed_line <= function.end_line
        })
    });
    let judged: Vec<(String, Form)> = match owner {
        Some(owner) => judged_for_owner(finding, owner, index),
        None => generated_owner(finding, index).into_iter().collect(),
    };
    if judged.is_empty() {
        return false;
    }
    let labels: Vec<String> = judged.iter().map(|(label, _)| label.clone()).collect();
    let mut forms = Vec::<Form>::new();
    for (_, form) in &judged {
        if !forms.contains(form) {
            forms.push(*form);
        }
    }
    let next_step = format!(
        "{} {}, so ripr does not report a gap. Check that the assertion's expected value differs between the old and the new behavior; add a test only if it does not.",
        labels.join(", "),
        forms
            .iter()
            .map(|form| form.advice())
            .collect::<Vec<_>>()
            .join("; and ")
    );
    withhold_gap(finding, LIMIT, FIRST_UNRESOLVED_EDGE, next_step, &labels);
    true
}

fn label(test: &TestFact) -> String {
    format!(
        "`{}` ({}:{})",
        test.name,
        test.file.display().to_string().replace('\\', "/"),
        test.start_line
    )
}

fn judged_for_owner(
    finding: &Finding,
    owner: &FunctionFact,
    index: &RustIndex,
) -> Vec<(String, Form)> {
    let mut judged = Vec::new();
    let is_related = |test: &TestFact, direct_only: bool| {
        finding.related_tests.iter().any(|related| {
            related.name == test.name
                && related.file == test.file
                && related.line == test.start_line
                && (!direct_only
                    || related.relation_reason == Some(RelationReason::DirectOwnerCall))
        })
    };
    // When some related test already calls the owner, reach is not in doubt:
    // a value mention only counts in a test the finding itself lists, so an
    // unrelated test that binds a same-named local cannot hide the gap of the
    // test that does call it. With proximity-only reach (the usual shape of
    // `.map(owner)`), every test that can see the owner is a candidate, since
    // the related list is capped.
    let called = finding.related_tests.iter().any(|related| {
        matches!(
            related.relation_reason,
            Some(RelationReason::DirectOwnerCall | RelationReason::HelperOwnerCall)
        )
    });
    for test in index.tests() {
        if is_ignored(test) || test.assertions.is_empty() {
            continue;
        }
        let code = mask_comments_and_strings(&test.body);
        // A test that names the owner as a value is a candidate only where it
        // can see the owner: its own file, or an integration test.
        let sees_owner = test.file == owner.file || is_test_file(&test.file);
        if sees_owner
            && (!called || is_related(test, false))
            && names_owner_as_value(&code, owner, test, index)
        {
            judged.push((label(test), Form::FunctionValue));
            continue;
        }
        let related_direct = is_related(test, true);
        // A sync owner's bare call is its value, not a future an executor
        // drives: `block_on(owner(..))` on one stays a plain gap (#10143).
        if related_direct
            && super::owner_pin::declares_async(owner, index)
            && executor_wraps_owner_call(&code, &owner.name, test, index)
        {
            judged.push((label(test), Form::ExecutorWrapped));
        }
    }
    judged
}

/// The changed line is an item-position macro invocation: no function in its
/// file holds the line, and the changed expression is `name!(..)`. The tests
/// named are the related ones that call a name the invocation spells.
fn generated_owner(finding: &Finding, index: &RustIndex) -> Option<(String, Form)> {
    let line = finding.probe.location.line;
    let file = &finding.probe.location.file;
    let inside_function = index.functions().iter().any(|function| {
        function.file == *file && function.start_line <= line && line <= function.end_line
    });
    if inside_function {
        return None;
    }
    let expression = mask_comments_and_strings(finding.probe.expression.trim());
    let expression = expression.trim().trim_end_matches(';').trim_end();
    let name_end = expression.find('!')?;
    let name = &expression[..name_end];
    if !is_path(name) {
        return None;
    }
    let rest = expression[name_end + 1..].trim_start();
    if !rest.starts_with(['(', '[', '{']) {
        return None;
    }
    let related = finding
        .related_tests
        .iter()
        .find(|related| related.relation_reason == Some(RelationReason::DirectOwnerCall))
        .or_else(|| finding.related_tests.first())?;
    let test = index.tests().iter().find(|test| {
        test.name == related.name && test.file == related.file && test.start_line == related.line
    })?;
    Some((label(test), Form::Generated))
}

fn is_ident(text: &str) -> bool {
    !text.is_empty()
        && !text.starts_with(|ch: char| ch.is_ascii_digit())
        && text.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
}

fn is_path(text: &str) -> bool {
    !text.is_empty() && text.split("::").all(is_ident)
}

fn is_ident_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// Offsets at which `name` appears as a whole identifier in `code`.
fn identifier_offsets<'a>(code: &'a str, name: &'a str) -> impl Iterator<Item = usize> + 'a {
    code.match_indices(name)
        .map(|(at, _)| at)
        .filter(move |at| {
            code[..*at]
                .chars()
                .next_back()
                .is_none_or(|ch| !is_ident_char(ch))
                && code[at + name.len()..]
                    .chars()
                    .next()
                    .is_none_or(|ch| !is_ident_char(ch))
        })
}

/// The test's code names `name` as a value: not called, not a macro, not a
/// method, field label or definition, the test binds no local of that name,
/// and a qualifying path (`other::inc`) can still name the owner.
fn names_owner_as_value(
    code: &str,
    owner: &FunctionFact,
    test: &TestFact,
    index: &RustIndex,
) -> bool {
    use super::call_identity::{CallTarget, shared_owner_call_identity};

    let identity = shared_owner_call_identity(index, owner);
    names_value(
        &test.body,
        code,
        &owner.name,
        &owner_path_names(owner),
        |path| {
            let Some((leaf, qualifier)) = path.split_last() else {
                return false;
            };
            leaf == &owner.name
                && identity.as_ref().is_some_and(|identity| {
                    identity.resolve_call(test, index, qualifier) == CallTarget::Other
                })
        },
    )
}

fn names_value(
    source: &str,
    code: &str,
    name: &str,
    owner_names: &[String],
    import_is_other: impl Fn(&[String]) -> bool,
) -> bool {
    if is_bound_locally(code, name) {
        return false;
    }
    let items = ItemBindings::read(source, name, import_is_other);
    identifier_offsets(code, name).any(|at| {
        let reference_at = code[..at].strip_suffix("r#").map_or(at, str::len);
        let before = code[..reference_at].trim_end();
        if items.declaration_at(at) || (!before.ends_with("::") && items.shadows_at(at)) {
            return false;
        }
        let after = code[at + name.len()..].trim_start();
        let defined = before.ends_with("fn") && ends_at_boundary(&before[..before.len() - 2]);
        let method = before.ends_with('.');
        let called = after.starts_with(['(', '!']) || after.starts_with("::<");
        let field_label = after.starts_with(':') && !after.starts_with("::");
        !(defined
            || method
            || called
            || field_label
            || typed_fn_pointer_initializer(code, reference_at)
            || !qualifier_may_name_owner(before, owner_names))
    })
}

/// Path segments that identify where the owner lives: `self`, `super` and
/// `crate`, the components of its file path (module and crate directory names,
/// `-` read as `_`), and the self type and trait of an impl it sits in.
fn owner_path_names(owner: &FunctionFact) -> Vec<String> {
    let mut names: Vec<String> = ["self", "super", "crate", "Self"]
        .into_iter()
        .map(String::from)
        .collect();
    for component in owner.file.components() {
        if let Some(text) = component.as_os_str().to_str() {
            let stem = text.strip_suffix(".rs").unwrap_or(text);
            names.push(stem.replace('-', "_"));
        }
    }
    let mut add_path = |path: &str| {
        names.extend(
            path.split(|ch: char| !is_ident_char(ch))
                .filter(|segment| !segment.is_empty())
                .map(String::from),
        );
    };
    match &owner.item.container {
        FunctionContainer::Inherent { self_ty } => add_path(self_ty),
        FunctionContainer::TraitImpl {
            trait_path,
            self_ty,
        } => {
            add_path(trait_path);
            add_path(self_ty);
        }
        _ => {}
    }
    names
}

/// Whether the path written before the owner's name (`other::` in
/// `other::inc`) can still be the owner's: no qualifier, or one whose first or
/// last segment is among the names that locate the owner. A qualifier that
/// names neither is a different entity with the same leaf name.
fn qualifier_may_name_owner(before: &str, owner_names: &[String]) -> bool {
    let Some(path) = before.strip_suffix("::") else {
        return true;
    };
    let start = path
        .rfind(|ch: char| !(is_ident_char(ch) || ch == ':'))
        .map_or(0, |at| at + 1);
    let segments: Vec<&str> = path[start..]
        .split("::")
        .filter(|segment| !segment.is_empty())
        .collect();
    let known = |segment: &&str| owner_names.iter().any(|name| name == segment);
    match (segments.first(), segments.last()) {
        (Some(first), Some(last)) => known(first) || known(last),
        // A leading `::` or a generic qualifier (`<T as Trait>::inc`) is not read.
        _ => false,
    }
}

/// The mention is the initializer of a `let f: fn(..) -> .. = owner;` binding.
/// The typed function-pointer reading follows that binding to its calls and
/// either credits or keeps the gap on the evidence (RIPR-SPEC-0115, #9478), so
/// this limit leaves the shape to it.
fn typed_fn_pointer_initializer(code: &str, at: usize) -> bool {
    let mut before = code[..at].trim_end();
    // Drop a path prefix such as `crate::` or `self::tests::`.
    while let Some(rest) = before.strip_suffix("::") {
        let rest = rest.trim_end();
        let start = rest
            .rfind(|ch: char| !is_ident_char(ch))
            .map_or(0, |index| index + 1);
        before = rest[..start].trim_end();
    }
    let Some(before) = before.strip_suffix('=') else {
        return false;
    };
    if before.ends_with(['=', '!', '<', '>']) {
        return false;
    }
    let Some(let_at) = identifier_offsets(before, "let").last() else {
        return false;
    };
    let binding = &before[let_at + 3..];
    binding
        .find(':')
        .is_some_and(|colon| binding[colon + 1..].trim_start().starts_with("fn("))
}

fn ends_at_boundary(text: &str) -> bool {
    text.chars().next_back().is_none_or(|ch| !is_ident_char(ch))
}

/// Whether `name` is bound as a local anywhere in the test: a typed or
/// untyped pattern after `let` (`let name`, `let (name, n)`, `if let
/// Some(name) =`), after `for` (`for (name, _) in`), a closure parameter
/// list (`|name|`), a `match` arm pattern (`Some(name) =>`) or a typed
/// parameter. A later mention is then the local, not the owner.
fn is_bound_locally(code: &str, name: &str) -> bool {
    pattern_spans(code)
        .into_iter()
        .any(|(start, end)| identifier_offsets(&code[start..end], name).next().is_some())
        || identifier_offsets(code, name).any(|at| {
            let after = code[at + name.len()..].trim_start();
            after.starts_with(':')
                && !after.starts_with("::")
                && code[..at].trim_end().ends_with(['(', ',', '|'])
        })
}

/// Declaration ranges and hoisted item bindings in the test's real lexical
/// blocks. Macro arguments remain text: their function values are token trees,
/// not `PathExpr` nodes. An unreadable parse cannot prove a shadow.
#[derive(Default)]
struct ItemBindings {
    declarations: Vec<ra_ap_syntax::TextRange>,
    scopes: Vec<(ra_ap_syntax::TextRange, bool)>,
}

impl ItemBindings {
    fn read(source: &str, name: &str, import_is_other: impl Fn(&[String]) -> bool) -> Self {
        use crate::analysis::syntax::{UseBinding, parse_clean_source_file, use_item_bindings};
        use ra_ap_syntax::{
            AstNode,
            ast::{self, HasAttrs, HasName},
        };

        let Some(parse) = parse_clean_source_file(source) else {
            return Self::default();
        };
        let mut result = Self::default();
        for node in parse.tree().syntax().descendants() {
            // An item scope cannot inherit the surrounding function's local
            // shadow. Its imports also need their own module context; leave
            // that identity unresolved instead of borrowing the test's.
            if ast::ItemList::can_cast(node.kind()) {
                result.scopes.push((node.text_range(), false));
                continue;
            }
            let shadow = if let Some(function) = ast::Fn::cast(node.clone()) {
                let Some(declared) = function.name() else {
                    continue;
                };
                if declared.text().trim_start_matches("r#") != name {
                    continue;
                }
                result.declarations.push(declared.syntax().text_range());
                function.attrs().next().is_none()
            } else if let Some(import) = ast::Use::cast(node.clone()) {
                result.declarations.push(node.text_range());
                let scope = use_item_bindings(&import);
                let matching: Vec<_> = scope
                    .imports
                    .iter()
                    .filter(|import| import.bound.trim_start_matches("r#") == name)
                    .collect();
                if matching.is_empty() && scope.globs.is_empty() {
                    continue;
                }
                import.attrs().next().is_none()
                    && scope.globs.is_empty()
                    && !matching.is_empty()
                    && matching.iter().all(|import| match &import.binding {
                        // Absence from a lexical owner-name list is not
                        // evidence of a different definition: reexports and
                        // unreadable module layouts retain the limit.
                        UseBinding::Path(path) | UseBinding::Renamed(path) => import_is_other(path),
                        UseBinding::Opaque => false,
                    })
            } else {
                continue;
            };
            // Items in a nested module or impl belong to its ItemList, not the
            // surrounding test. Never climb across that boundary.
            if let Some(block) = node.parent().and_then(ast::StmtList::cast) {
                result.scopes.push((block.syntax().text_range(), shadow));
            }
        }
        result
    }

    fn declaration_at(&self, at: usize) -> bool {
        self.declarations
            .iter()
            .any(|range| contains_offset(*range, at))
    }

    fn shadows_at(&self, at: usize) -> bool {
        let Some((innermost, _)) = self
            .scopes
            .iter()
            .filter(|(range, _)| contains_offset(*range, at))
            .min_by_key(|(range, _)| range.len())
        else {
            return false;
        };
        // A nearer owner-preserving import overrides an outer shadow. An
        // opaque, conditional or competing binding cannot prove another owner.
        self.scopes
            .iter()
            .filter(|(range, _)| range == innermost)
            .all(|(_, shadow)| *shadow)
    }
}

fn contains_offset(range: ra_ap_syntax::TextRange, at: usize) -> bool {
    usize::from(range.start()) <= at && at < usize::from(range.end())
}

/// Byte ranges of `code` that are binding patterns: from `let`/`for` to the
/// next `=` or `in`, closure parameter lists between `|` bars, and match-arm
/// patterns from the previous `{`, `,` or `;` to `=>`.
fn pattern_spans(code: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    for keyword in ["let", "for"] {
        for at in identifier_offsets(code, keyword) {
            let from = at + keyword.len();
            let rest = &code[from..];
            let stop = if keyword == "let" {
                rest.find(['=', ';'])
            } else {
                rest.find(" in ").or_else(|| rest.find('{'))
            };
            if let Some(stop) = stop {
                spans.push((from, from + stop));
            }
        }
    }
    let bytes = code.as_bytes();
    let mut open = None;
    for (at, byte) in bytes.iter().enumerate() {
        if *byte == b'|'
            && bytes.get(at + 1) != Some(&b'|')
            && at.checked_sub(1).map(|p| bytes[p]) != Some(b'|')
        {
            match open.take() {
                Some(start) => spans.push((start, at)),
                None => open = Some(at + 1),
            }
        }
    }
    for (at, _) in code.match_indices("=>") {
        let start = code[..at]
            .rfind(['{', ',', ';'])
            .map_or(0, |found| found + 1);
        spans.push((start, at));
    }
    spans
}

/// The owner is called as the only argument of an executor: a local helper
/// whose signature mentions `Future`, or a callee named like a known
/// executor, written bare, as a path or as a method.
fn executor_wraps_owner_call(code: &str, owner: &str, test: &TestFact, index: &RustIndex) -> bool {
    identifier_offsets(code, owner).any(|at| {
        let after = &code[at + owner.len()..];
        if !after.trim_start().starts_with('(') {
            return false;
        }
        let blanks = after.len() - after.trim_start().len();
        let Some(call_end) = call_end(code, at + owner.len() + blanks) else {
            return false;
        };
        // The owner call is the whole argument list of the wrapper.
        if !code[call_end..].trim_start().starts_with(')') {
            return false;
        }
        let before = code[..at].trim_end();
        let Some(open) = before.strip_suffix('(') else {
            return false;
        };
        let open = open.trim_end();
        let callee_start = open
            .rfind(|ch: char| !(is_ident_char(ch) || ch == ':'))
            .map_or(0, |at| at + 1);
        let callee = &open[callee_start..];
        let leaf = callee.rsplit("::").next().unwrap_or(callee);
        if !is_ident(leaf) || leaf == owner {
            return false;
        }
        let defined_locally = callee == leaf
            && !open[..callee_start].trim_end().ends_with('.')
            && index
                .functions()
                .iter()
                .any(|function| function.name == leaf && function.file == test.file);
        if defined_locally {
            // A helper of the test's own file counts only when it takes a
            // `Future`: a same-named synchronous identity wrapper is not an
            // executor, and a literal expectation through a helper is also
            // read by `framework_oracle` (#10143), which reaches the same
            // withhold. A non-literal expectation falls only to this rule.
            return index.functions().iter().any(|function| {
                function.name == leaf
                    && function.file == test.file
                    && signature(function).contains("Future")
            });
        }
        KNOWN_EXECUTORS.contains(&leaf)
    })
}

/// The text of a function up to its body's opening brace.
fn signature(function: &FunctionFact) -> String {
    let body: &str = &function.body;
    let masked = mask_comments_and_strings(body);
    masked
        .find('{')
        .map_or(masked.as_str(), |open| &masked[..open])
        .to_string()
}

/// Offset just past the `)` that closes the call whose `(` starts at `open`.
fn call_end(code: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, ch) in code[open..].char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(open + offset + ch.len_utf8());
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner_names() -> Vec<String> {
        ["self", "super", "crate", "Self", "src", "lib"]
            .into_iter()
            .map(String::from)
            .collect()
    }

    fn value(code: &str, name: &str) -> bool {
        let source = format!("fn test_body() {{ {code} }}");
        names_value(
            &source,
            &mask_comments_and_strings(&source),
            name,
            &owner_names(),
            |_| false,
        )
    }

    #[test]
    fn a_path_value_is_named_and_a_call_is_not() {
        let code = "assert_eq!(v.into_iter().map(inc).collect::<Vec<_>>(), w);";
        assert!(value(code, "inc"));
        assert!(!value("assert_eq!(inc(1), 2);", "inc"));
        assert!(!value("assert_eq!(inc (1), 2);", "inc"));
        assert!(!value("inc!(1);", "inc"));
        assert!(!value("x.inc;", "inc"));
        assert!(!value("let inc = 3; use_it(inc);", "inc"));
        assert!(!value("use super::inc;\n", "inc"));
        assert!(!value("S { inc: 1 };", "inc"));
        assert!(!value("let increment = 3;", "inc"));
    }

    #[test]
    fn a_qualified_path_names_the_owner_only_when_it_locates_the_owner() {
        // A same-leaf function in another module is a different entity.
        assert!(!value("assert_eq!([1, 2].map(other::inc), w);", "inc"));
        assert!(!value("f(a::b::inc);", "inc"));
        assert!(!value("f(<T as Tr>::inc);", "inc"));
        assert!(!value("f(::inc);", "inc"));
        // The owner's own module, `super`, `self`, `crate` and the owner's file stem.
        assert!(value("assert_eq!([1, 2].map(super::inc), w);", "inc"));
        assert!(value("assert_eq!([1, 2].map(self::inc), w);", "inc"));
        assert!(value("assert_eq!([1, 2].map(crate::lib::inc), w);", "inc"));
        assert!(value("assert_eq!([1, 2].map(lib::inc), w);", "inc"));
        // One qualified other-module mention does not hide a bare value mention.
        assert!(value("f(other::inc); g(inc);", "inc"));
    }

    #[test]
    fn an_untyped_function_pointer_binding_is_a_value() {
        let code = "let f = call_through_pointer; assert_eq!(f(2), 6);";
        assert!(value(code, "call_through_pointer"));
    }

    #[test]
    fn a_typed_function_pointer_binding_is_left_to_the_typed_reading() {
        for code in [
            "let f: fn(u32) -> u32 = call_through_pointer; assert_eq!(f(2), 6);",
            "let f: fn(u32) -> u32 = crate::call_through_pointer; assert_eq!(f(2), 6);",
        ] {
            assert!(!value(code, "call_through_pointer"), "{code}");
        }
    }

    #[test]
    fn a_local_with_the_owner_name_is_not_the_owner() {
        assert!(!value("let mut inc = 1; inc += 1; keep(inc);", "inc"));
        assert!(!value("let go = |inc: u32| inc + 1; go(1);", "inc"));
    }

    #[test]
    fn pattern_bindings_of_the_owner_name_are_locals() {
        for code in [
            "if let Some(total) = r { keep(total); }",
            "for (total, _) in rows { keep(total); }",
            "let (total, n) = pair; keep(total);",
            "let go = |total| total + 1; go(1);",
            "match r { Ok(total) => keep(total), Err(_) => {} }",
        ] {
            assert!(!value(code, "total"), "{code}");
        }
        assert!(value("rows.map(total).count();", "total"));
    }

    #[test]
    fn call_end_matches_nested_delimiters() {
        let code = "add(f(1, 2), [3]) + 1";
        assert_eq!(call_end(code, 3), Some(17));
        assert_eq!(call_end("add(1", 3), None);
    }

    #[test]
    fn item_macros_need_a_path_and_a_delimiter() {
        assert!(is_path("def_add"));
        assert!(is_path("crate::def_add"));
        assert!(!is_path("2 + 3"));
        assert!(!is_path(""));
    }
}
