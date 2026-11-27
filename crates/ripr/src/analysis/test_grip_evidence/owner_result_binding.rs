//! Parser-backed owner-result binding for FieldConstruction missing facts.
//!
//! A compatible `RequiredDiscriminator::FieldValue` fact is emitted only
//! when activation is already known and a unique parser-backed test binds
//! a plain local to a *direct* captured owner call, then weakly observes
//! that same binding's constructed field. Presence of a nearby test or
//! field name never manufactures activation. A grouped nested-`super`
//! import counts as that owner only when the resolved module uniquely
//! matches this seam's owner; `super::` itself is not a whitelist, and a
//! leading `::` extern-prelude path is not a local module.
//!
//! #13700: the same fact also starts when the *only* related test binds the
//! owner result exactly once and never reads the changed field at all: every
//! later mention of the binding is a read of a different named field, the
//! binding is never shadowed, passed, matched or formatted, the field name
//! appears nowhere in the test, and the field's declared visibility reaches
//! the test file. Any read of the field (strong or weak) keeps the earlier
//! reading; anything this token reading cannot place withholds the fact.

use super::record_field_name;
use super::related_tests::call_text_contains_named_call;
use super::related_tests::context::{CompactGripContext, CompactTest};
use super::related_tests::module_path_for_index;
use crate::analysis::rust_index::{FunctionSummary, RustIndex, TestSummary};
use crate::analysis::seams::{RepoSeam, RequiredDiscriminator, SeamKind};
use crate::analysis::syntax::parse_clean_source_file;
use crate::analysis::syntax::ra::LineIndex;
use crate::domain::{
    DiscriminatorShape, MissingDiscriminatorFact, OracleKind, OracleStrength, StageState,
};
use ra_ap_syntax::ast::{self, HasName};
use ra_ap_syntax::{AstNode, AstPtr, Parse, SourceFile, SyntaxNode};
use std::collections::BTreeMap;
use std::path::Path;

pub(super) fn missing_field_value_facts(
    seam: &RepoSeam,
    related: &[&CompactTest<'_>],
    context: &CompactGripContext<'_>,
    owner_fn: Option<&FunctionSummary>,
    activation: &StageState,
) -> Vec<MissingDiscriminatorFact> {
    if *activation != StageState::Yes || seam.kind() != SeamKind::FieldConstruction {
        return Vec::new();
    }
    // #11398: a field write names the written field, not an owner-result
    // field. #11870: it starts a repair only through the one weak-read policy
    // that `check` also asks, over the tests that run the owner; the caller
    // that has each test's relation asks it ([`assigned_field_weak_read_fact`]).
    if crate::analysis::classify::field_assignment_target(seam.expression()).is_some() {
        return Vec::new();
    }
    let Some(owner_fn) = owner_fn else {
        return Vec::new();
    };
    let owner_name = owner_fn.name.as_str();
    if owner_name.is_empty() {
        return Vec::new();
    }
    let owner_module = module_path_for_index(context.index, &owner_fn.file);
    let RequiredDiscriminator::FieldValue { field } = seam.required_discriminator() else {
        return Vec::new();
    };
    let Some(field_name) = record_field_name(field) else {
        return Vec::new();
    };

    let mut saw_strong = false;
    let mut saw_weak = false;
    let mut unread = 0usize;
    for indexed in related {
        match owner_result_field_observation(
            indexed,
            context,
            owner_fn,
            owner_module.as_deref(),
            field_name,
        ) {
            Some(OwnerResultObservation::Strong) => saw_strong = true,
            Some(OwnerResultObservation::Weak) => saw_weak = true,
            Some(OwnerResultObservation::Unread) => unread += 1,
            None => {}
        }
        if saw_strong {
            break;
        }
    }
    if saw_strong {
        return Vec::new();
    }
    let reason = if saw_weak {
        format!(
            "owner-result field `{field_name}` is observed only by a weak oracle; an exact field-value discriminator is missing"
        )
    } else if let [only] = related
        && unread == 1
        && constructed_field_visible_from(seam, owner_fn, field_name, &only.test.file, context)
    {
        // #13700: one deterministic target. A second related test, or a
        // field the test file cannot name, withholds the fact.
        format!(
            "owner-result field `{field_name}` is bound but never read by the related test; an exact field-value discriminator is missing"
        )
    } else {
        return Vec::new();
    };

    vec![MissingDiscriminatorFact {
        value: field.clone(),
        reason,
        flow_sink: None,
        shape: DiscriminatorShape::Other,
        drafted_boundary: None,
    }]
}

/// #11870: the missing-discriminator fact for a changed `self.f = ..` write
/// that a related test reads with a relational or `assert_ne!` check only.
/// Fail-closed: the policy is [`assigned_field_weak_read_start`] (which also
/// withholds a non-plain `Drop` owner type); an owner the seam cannot place
/// withholds it.
pub(super) fn assigned_field_weak_read_fact(
    seam: &RepoSeam,
    reaching: &[&crate::analysis::rust_index::TestSummary],
    context: &CompactGripContext<'_>,
    owner_fn: Option<&FunctionSummary>,
) -> Vec<MissingDiscriminatorFact> {
    let (Some(owner), Some(field)) = (owner_fn, super::written_field(seam)) else {
        return Vec::new();
    };
    let tests = reaching;
    if !crate::analysis::classify::assigned_field_weak_read_start(
        owner,
        seam.expression(),
        field,
        tests,
        context.index,
    ) {
        return Vec::new();
    }
    vec![MissingDiscriminatorFact {
        value: field.to_string(),
        reason: format!(
            "written field `{field}` is read only by a weak check; an exact field-value discriminator is missing"
        ),
        flow_sink: None,
        shape: DiscriminatorShape::Other,
        drafted_boundary: None,
    }]
}

enum OwnerResultObservation {
    Strong,
    Weak,
    /// #13700: one direct owner-result binding whose changed field the test
    /// never names (see [`binding_never_reads_field`]).
    Unread,
}

fn owner_result_field_observation(
    indexed: &CompactTest<'_>,
    context: &CompactGripContext<'_>,
    owner_fn: &FunctionSummary,
    owner_module: Option<&str>,
    field_name: &str,
) -> Option<OwnerResultObservation> {
    let owner_name = owner_fn.name.as_str();
    if !indexed.test.calls.iter().any(|call| {
        call.name == owner_name
            && call_text_contains_named_call(&call.text, owner_name)
            && (owner_fn.impl_context != crate::analysis::facts::FunctionImplContext::Free
                || crate::analysis::classify::call_text_may_call_free_function(
                    &call.text, owner_name,
                ))
    }) {
        return None;
    }
    let facts = context.index.files().get(indexed.test.file.as_path())?;
    if facts.used_lexical_fallback {
        return None;
    }
    let _ = context.unique_evidence_function(
        &indexed.test.file,
        &indexed.test.name,
        indexed.test.start_line,
    )?;
    let parsed = context.parsed_test_file(&indexed.test.file)?;
    let lines = &parsed.lines;
    let function = parsed.unique_test_fn(&indexed.test.name, indexed.test.start_line)?;
    if owner_callee_is_ambiguous(
        &function,
        owner_fn,
        indexed.test,
        owner_module,
        context.index,
    ) {
        return None;
    }
    let bindings = direct_owner_result_bindings(&function, owner_name, lines);
    if bindings.is_empty() {
        return None;
    }
    let mut kind = None;
    for oracle in &indexed.test.assertions {
        for binding in &bindings {
            if oracle.line <= binding.line {
                continue;
            }
            if binding_invalidated_before(&function, binding, oracle.line, lines) {
                continue;
            }
            if !oracle_observes_binding_field(&oracle.text, &binding.name, field_name) {
                continue;
            }
            match observation_from_oracle(oracle.kind.clone(), oracle.strength.clone()) {
                Some(OwnerResultObservation::Strong) => {
                    return Some(OwnerResultObservation::Strong);
                }
                Some(OwnerResultObservation::Weak) => kind = Some(OwnerResultObservation::Weak),
                Some(OwnerResultObservation::Unread) | None => {}
            }
        }
    }
    if kind.is_none()
        && let [binding] = bindings.as_slice()
        && let Some(reads) = binding_never_reads_field(&function, binding, owner_name, field_name)
        && !file_renames_owner(&parsed.parse, owner_name)
        // The owner result must reach an oracle: one accepted
        // `binding.<other_field>` read appears in an assertion's condition or
        // compared operands after the binding. A raw word (a string literal, a
        // comment, another value's `.c`) is not a read, and a binding no
        // assertion reads (`let _c = owner(); assert!(1 > 0);`) observes
        // nothing, so it stays non-ready.
        && reads.iter().any(|read| {
            read_in_assertion_after_binding(&function, *read, binding, indexed, lines)
        })
    {
        return Some(OwnerResultObservation::Unread);
    }
    kind
}

/// #13700: whether the test never reads `field` through `binding` or any other
/// way the token stream can spell. Fail-closed token reading over the test
/// function's own tokens, macro bodies included: `binding` is declared once
/// (never shadowed); every other mention of it is a clean
/// `binding.<other_field>` read whose next token is a terminator from
/// [`clean_read_terminator`] (so never passed, matched, compared whole,
/// spread as `..binding`, method-called, turbofished, indexed, assigned or
/// chained into a sub-field such as `binding.inner.x`, which refuses
/// fail-closed); the owner's name appears nowhere outside the one binding
/// initializer (a second call, path or reference could observe the field
/// through whole-value equality or a snapshot); `field` is never named as an
/// identifier; and no string literal names `field` or captures `binding` in a
/// format argument. Only the test body is scanned: a helper, `const`, macro
/// or trait delegation (`Config::default()` calling the owner) defined
/// outside it is not read, and neither is an alias re-exported from the
/// production crate (`pub use config::default_config as dc;`), so a call to
/// the owner made through any of them is not seen. A `use` alias of the
/// owner in the test file itself is refused separately
/// ([`file_renames_owner`]).
///
/// Answers the ranges of the accepted `binding.<other_field>` reads (the
/// binding-name tokens), or `None` when any token refuses the shape.
fn binding_never_reads_field(
    function: &ast::Fn,
    binding: &OwnerResultBinding,
    owner_name: &str,
    field: &str,
) -> Option<Vec<ra_ap_syntax::TextRange>> {
    use ra_ap_syntax::SyntaxKind;
    let declarations = function
        .syntax()
        .descendants()
        .filter_map(ast::IdentPat::cast)
        .filter(|pattern| {
            pattern
                .name()
                .is_some_and(|name| name.text() == binding.name)
        })
        .count();
    if declarations != 1 {
        return None;
    }
    let tokens: Vec<_> = function
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    let capture = format!("{{{}", binding.name);
    let mut reads = Vec::new();
    for (at, token) in tokens.iter().enumerate() {
        match token.kind() {
            SyntaxKind::STRING | SyntaxKind::BYTE_STRING | SyntaxKind::C_STRING => {
                let text = token.text();
                if crate::analysis::classify::mentions_in(text, field) > 0
                    || text.contains(&capture)
                {
                    return None;
                }
            }
            SyntaxKind::IDENT => {
                let text = token.text();
                if text.trim_start_matches("r#") == field {
                    return None;
                }
                if text.trim_start_matches("r#") == owner_name
                    && !binding.initializer_range.contains_range(token.text_range())
                {
                    // A second owner call, path or reference may observe the
                    // field through the whole value.
                    return None;
                }
                if text != binding.name || token.text_range() == binding.name_range {
                    continue;
                }
                let kind_at = |index: Option<usize>| {
                    index
                        .and_then(|index| tokens.get(index))
                        .map(|token| token.kind())
                };
                if kind_at(at.checked_sub(1)) == Some(SyntaxKind::DOT) {
                    // Inside a macro token tree `..` is `DOT DOT`, so
                    // `..binding` (a struct-update base carrying the whole
                    // value) refuses; a single dot is `other.<name>`, a
                    // field of something else.
                    if kind_at(at.checked_sub(2)) == Some(SyntaxKind::DOT) {
                        return None;
                    }
                    continue;
                }
                let other_field_read = kind_at(Some(at + 1)) == Some(SyntaxKind::DOT)
                    && kind_at(Some(at + 2)) == Some(SyntaxKind::IDENT)
                    && clean_read_terminator(kind_at(Some(at + 3)), kind_at(Some(at + 4)));
                if !other_field_read {
                    return None;
                }
                reads.push(token.text_range());
            }
            _ => {}
        }
    }
    Some(reads)
}

/// #13700: whether the token after a `binding.<name>` read (`next`, with
/// `after` the one following it) ends a plain field read. Inside a macro
/// token tree compound punctuation is split (`==` is `EQ EQ`, `::` is
/// `COLON COLON`), so both spellings are listed. Everything else refuses:
/// a call or turbofish (`(`, `::`, `:`), an index (`[`), a further
/// `.`, an assignment (`=` alone or a compound `+=`), a shift, a cast or
/// a `?`.
fn clean_read_terminator(
    next: Option<ra_ap_syntax::SyntaxKind>,
    after: Option<ra_ap_syntax::SyntaxKind>,
) -> bool {
    use ra_ap_syntax::SyntaxKind;
    let Some(next) = next else {
        return false;
    };
    match next {
        SyntaxKind::R_PAREN
        | SyntaxKind::R_BRACK
        | SyntaxKind::R_CURLY
        | SyntaxKind::COMMA
        | SyntaxKind::SEMICOLON
        | SyntaxKind::EQ2
        | SyntaxKind::NEQ
        | SyntaxKind::LTEQ
        | SyntaxKind::GTEQ
        | SyntaxKind::AMP2
        | SyntaxKind::PIPE2 => true,
        // Split `==` / `!=` inside a token tree; a lone `=` assigns.
        SyntaxKind::EQ | SyntaxKind::BANG => after == Some(SyntaxKind::EQ),
        SyntaxKind::AMP => after == Some(SyntaxKind::AMP),
        SyntaxKind::PIPE => after == Some(SyntaxKind::PIPE),
        // `<`, `>`, `<=`, `>=`; a doubled angle is a shift (or `<<=`).
        SyntaxKind::L_ANGLE | SyntaxKind::R_ANGLE => after != Some(next),
        // Arithmetic, but not a split compound assignment (`+ =`).
        SyntaxKind::PLUS
        | SyntaxKind::MINUS
        | SyntaxKind::STAR
        | SyntaxKind::SLASH
        | SyntaxKind::PERCENT => after != Some(SyntaxKind::EQ),
        _ => false,
    }
}

/// #13700: whether the accepted binding read at `read` appears in the
/// condition of an `assert!` or in the two compared operands of an
/// `assert_eq!`/`assert_ne!` (not that it decides the assertion), whose
/// macro call
/// starts on an indexed assertion line after the binding. Format arguments,
/// other macros and reads outside any assertion do not count.
fn read_in_assertion_after_binding(
    function: &ast::Fn,
    read: ra_ap_syntax::TextRange,
    binding: &OwnerResultBinding,
    indexed: &CompactTest<'_>,
    lines: &LineIndex,
) -> bool {
    use ra_ap_syntax::SyntaxKind;
    let ra_ap_syntax::NodeOrToken::Token(token) = function.syntax().covering_element(read) else {
        return false;
    };
    let Some(call) = token.parent_ancestors().find_map(ast::MacroCall::cast) else {
        return false;
    };
    let Some(macro_name) = call
        .path()
        .and_then(|path| path.segment())
        .and_then(|segment| segment.name_ref())
        .map(|name| name.text().to_string())
    else {
        return false;
    };
    let allowed_commas = match macro_name.as_str() {
        "assert" => 0,
        "assert_eq" | "assert_ne" => 1,
        _ => return false,
    };
    let Some(tree) = call.token_tree() else {
        return false;
    };
    // Count only the call's top-level commas before the read: commas inside
    // a nested group (`(a, b)`, `vec![a, b]`) belong to that group. A
    // top-level comma that is not an argument separator (`HashMap<K, V>`)
    // over-counts, which refuses (fail closed).
    let commas_before = tree
        .syntax()
        .children_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|comma| {
            comma.kind() == SyntaxKind::COMMA && comma.text_range().end() <= read.start()
        })
        .count();
    if commas_before > allowed_commas {
        return false;
    }
    let call_line = lines.line(call.syntax().text_range().start());
    call_line > binding.line
        && indexed
            .test
            .assertions
            .iter()
            .any(|oracle| oracle.line == call_line)
}

/// #13700: the owner's declared return type name (generics stripped) when
/// `record` is the owner function's top-level return construction: its body's
/// tail expression or the value of a `return` directly in the owner (not in a
/// closure, and not in an `async`, `const`, `try` or `gen` block, whose
/// `return` leaves that block rather than the owner), parentheses allowed.
/// The enclosing function must be the owner's own node (`owner_node`), not
/// a same-named nested `fn`. A nested initializer, a call argument or any
/// other position answers `None`.
fn owner_top_level_return_type(record: &ast::RecordExpr, owner_node: &ast::Fn) -> Option<String> {
    let mut outer = record.syntax().clone();
    while let Some(parent) = outer
        .parent()
        .filter(|parent| ast::ParenExpr::can_cast(parent.kind()))
    {
        outer = parent;
    }
    let parent = outer.parent()?;
    let function = if let Some(stmt_list) = ast::StmtList::cast(parent.clone()) {
        if stmt_list.tail_expr()?.syntax() != &outer {
            return None;
        }
        let body = ast::BlockExpr::cast(stmt_list.syntax().parent()?)?;
        ast::Fn::cast(body.syntax().parent()?)?
    } else if ast::ReturnExpr::can_cast(parent.kind()) {
        let enclosing = parent.ancestors().find(|node| {
            ast::Fn::can_cast(node.kind())
                || ast::ClosureExpr::can_cast(node.kind())
                || ast::BlockExpr::cast(node.clone()).is_some_and(|block| {
                    block.modifier().is_some_and(|modifier| {
                        !matches!(
                            modifier,
                            ast::BlockModifier::Unsafe(_) | ast::BlockModifier::Label(_)
                        )
                    })
                })
        })?;
        ast::Fn::cast(enclosing)?
    } else {
        return None;
    };
    if function.syntax() != owner_node.syntax() {
        return None;
    }
    let ty = function.ret_type()?.ty()?.syntax().text().to_string();
    let name = ty.split('<').next().unwrap_or(&ty).trim().to_string();
    valid_identifier(&name).then_some(name)
}

/// #13700: whether any `use` item in the test file renames the owner's name
/// (`use fx::owner as o;`, grouped `use fx::{owner as o}`): a call through
/// the alias never spells the owner's name, so the token scan cannot see it.
fn file_renames_owner(parse: &Parse<SourceFile>, owner_name: &str) -> bool {
    parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::UseTree::cast)
        .any(|tree| {
            tree.rename().is_some()
                && tree
                    .path()
                    .and_then(|path| path.segment())
                    .and_then(|segment| segment.name_ref())
                    .is_some_and(|name| name.text().trim_start_matches("r#") == owner_name)
        })
}

/// #13700: whether the record literal that holds the changed field is the
/// owner's own top-level return construction (its tail expression or a
/// `return` value, never a nested initializer such as `Wrapper { inner:
/// Inner { .. } }`), names the owner's declared return type, names a struct
/// the owner's file declares exactly once with that field, and that field's
/// declared visibility ([`crate::analysis::classify::FieldTable`]) reaches
/// `test_file`. Anything this reading cannot place is not visible.
fn constructed_field_visible_from(
    seam: &RepoSeam,
    owner_fn: &FunctionSummary,
    field: &str,
    test_file: &Path,
    context: &CompactGripContext<'_>,
) -> bool {
    if seam.file() != owner_fn.file.as_path() {
        return false;
    }
    let Some(parsed) = context.parsed_test_file(seam.file()) else {
        return false;
    };
    let mut literals = parsed
        .parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::RecordExprField::cast)
        .filter(|entry| {
            entry
                .field_name()
                .is_some_and(|name| name.text().trim_start_matches("r#") == field)
                && parsed.lines.line(entry.syntax().text_range().start()) == seam.display_line()
        });
    let (Some(entry), None) = (literals.next(), literals.next()) else {
        return false;
    };
    let Some(record) = entry.syntax().ancestors().find_map(ast::RecordExpr::cast) else {
        return false;
    };
    let Some(owner_node) = parsed.unique_test_fn(&owner_fn.name, owner_fn.start_line) else {
        return false;
    };
    let Some(owner_return) = owner_top_level_return_type(&record, &owner_node) else {
        return false;
    };
    let Some(path) = record.path() else {
        return false;
    };
    let path = path.syntax().text().to_string();
    let type_name = if path == "Self" {
        let crate::analysis::facts::FunctionImplContext::Impl { self_type } =
            &owner_fn.impl_context
        else {
            return false;
        };
        self_type
            .split('<')
            .next()
            .unwrap_or(self_type)
            .trim()
            .to_string()
    } else {
        path
    };
    if !valid_identifier(&type_name) {
        return false;
    }
    // The constructed type must be the owner's declared return type (`Self`
    // there resolves to the impl type, as above).
    let owner_return = if owner_return == "Self" {
        match &owner_fn.impl_context {
            crate::analysis::facts::FunctionImplContext::Impl { self_type } => self_type
                .split('<')
                .next()
                .unwrap_or(self_type)
                .trim()
                .to_string(),
            _ => return false,
        }
    } else {
        owner_return
    };
    if owner_return != type_name {
        return false;
    }
    let Some(facts) = context.index.files().get(owner_fn.file.as_path()) else {
        return false;
    };
    let Some(reach) = crate::analysis::classify::FieldTable::parse(&facts.source)
        .and_then(|table| table.struct_field_reach(&type_name, field))
    else {
        return false;
    };
    crate::analysis::classify::test_can_name_field(reach, &owner_fn.file, test_file)
}

struct OwnerResultBinding {
    name: String,
    name_range: ra_ap_syntax::TextRange,
    line: usize,
    range_end: ra_ap_syntax::TextSize,
    /// The direct owner call this binding is initialized from.
    initializer_range: ra_ap_syntax::TextRange,
}

/// One parsed test file: its tree, line index, and test-function lookup.
/// Owner-result inspection runs once per seam and related test, so these
/// are built once per file instead of re-walking the whole tree each time.
pub(in crate::analysis::test_grip_evidence) struct ParsedTestFile {
    parse: Parse<SourceFile>,
    lines: LineIndex,
    /// `fn` items by (name, line of the `fn` token): the first in source
    /// order, or `None` when two or more share the key.
    functions: BTreeMap<(String, usize), Option<AstPtr<ast::Fn>>>,
}

impl ParsedTestFile {
    pub(in crate::analysis::test_grip_evidence) fn new(
        parse: Parse<SourceFile>,
        source: &str,
    ) -> Self {
        let lines = LineIndex::new(source);
        let mut functions = BTreeMap::new();
        for function in parse
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::Fn::cast)
        {
            let (Some(name), Some(token)) = (function.name(), function.fn_token()) else {
                continue;
            };
            let key = (
                name.text().to_string(),
                lines.line(token.text_range().start()),
            );
            functions
                .entry(key)
                .and_modify(|slot| *slot = None)
                .or_insert_with(|| Some(AstPtr::new(&function)));
        }
        Self {
            parse,
            lines,
            functions,
        }
    }

    /// The only `fn` named `name` whose `fn` token starts on `start_line`.
    pub(in crate::analysis::test_grip_evidence) fn unique_test_fn(
        &self,
        name: &str,
        start_line: usize,
    ) -> Option<ast::Fn> {
        let ptr = self
            .functions
            .get(&(name.to_string(), start_line))?
            .as_ref()?;
        Some(ptr.to_node(&self.parse.syntax_node()))
    }

    /// Exact top-level out-of-line declaration for the resolved module
    /// provenance consumer. Multiple declarations sharing the key are unknown.
    pub(in crate::analysis::test_grip_evidence) fn unique_module_declaration(
        &self,
        name: &str,
        line: usize,
    ) -> Option<ast::Module> {
        let mut matches = self
            .parse
            .tree()
            .syntax()
            .children()
            .filter_map(ast::Module::cast)
            .filter(|module| {
                module.item_list().is_none()
                    && module.name().is_some_and(|item| item.text() == name)
                    && module
                        .mod_token()
                        .is_some_and(|token| self.lines.line(token.text_range().start()) == line)
            });
        let module = matches.next()?;
        matches.next().is_none().then_some(module)
    }
}

fn owner_callee_is_ambiguous(
    function: &ast::Fn,
    owner_fn: &FunctionSummary,
    test: &TestSummary,
    owner_module: Option<&str>,
    index: &RustIndex,
) -> bool {
    let owner_name = owner_fn.name.as_str();
    nested_owner_fn(function, owner_name)
        || sibling_owner_fn(function, owner_name)
        || (foreign_owner_import(function, owner_name, &test.file, owner_module, index)
            // #13700: an import the module walk reads as foreign (an
            // integration test's `use package::owner`) still names the owner
            // when the shared call-identity authority settles the bare call
            // on it. Unsettled or rival readings stay ambiguous.
            && !crate::analysis::classify::bare_owner_call_settles_on_owner(index, owner_fn, test))
        || local_owner_binding(function, owner_name)
}

fn local_owner_binding(function: &ast::Fn, owner_name: &str) -> bool {
    function
        .syntax()
        .descendants()
        .filter_map(ast::IdentPat::cast)
        .any(|pattern| {
            pattern.name().is_some_and(|name| name.text() == owner_name)
                && !ident_pat_is_direct_owner_result(&pattern, owner_name)
        })
}

fn ident_pat_is_direct_owner_result(pattern: &ast::IdentPat, owner_name: &str) -> bool {
    pattern
        .syntax()
        .parent()
        .and_then(ast::LetStmt::cast)
        .is_some_and(|binding| {
            plain_binding_name(&binding).as_deref() == Some(owner_name)
                && binding.initializer().is_some_and(|initializer| {
                    initializer_is_direct_owner_call(&initializer, owner_name)
                })
        })
}

fn nested_owner_fn(function: &ast::Fn, owner_name: &str) -> bool {
    function
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
        .any(|nested| {
            nested.syntax().text_range() != function.syntax().text_range()
                && nested.name().is_some_and(|name| name.text() == owner_name)
        })
}

fn sibling_owner_fn(function: &ast::Fn, owner_name: &str) -> bool {
    let Some(container) = enclosing_item_container(function) else {
        return false;
    };
    if ast::SourceFile::can_cast(container.kind()) {
        return false;
    }
    item_children(&container)
        .into_iter()
        .filter_map(ast::Fn::cast)
        .any(|sibling| {
            sibling.syntax().text_range() != function.syntax().text_range()
                && sibling.name().is_some_and(|name| name.text() == owner_name)
        })
}

fn foreign_owner_import(
    function: &ast::Fn,
    owner_name: &str,
    test_file: &Path,
    owner_module: Option<&str>,
    index: &RustIndex,
) -> bool {
    let resolved = owner_module.and_then(|owner_module| {
        module_path_for_index(index, test_file).map(|file_module| (owner_module, file_module))
    });
    owner_scope_uses(function).any(|item| match &resolved {
        Some((owner_module, file_module)) => {
            let current_module = use_current_module(&item, file_module);
            use_binds_foreign_owner(&item, owner_name, &current_module, owner_module, index)
        }
        None => use_tree_binds_owner_name(item.use_tree().as_ref(), owner_name),
    })
}

fn owner_scope_uses(function: &ast::Fn) -> impl Iterator<Item = ast::Use> {
    let container_uses = enclosing_item_container(function)
        .into_iter()
        .flat_map(|container| {
            item_children(&container)
                .into_iter()
                .filter_map(ast::Use::cast)
        });
    let nested_uses = function.syntax().descendants().filter_map(ast::Use::cast);
    container_uses.chain(nested_uses)
}

fn use_tree_binds_owner_name(tree: Option<&ast::UseTree>, owner_name: &str) -> bool {
    let Some(tree) = tree else {
        return false;
    };
    if tree.star_token().is_some() {
        return false;
    }
    if let Some(list) = tree.use_tree_list() {
        return list
            .use_trees()
            .any(|nested| use_tree_binds_owner_name(Some(&nested), owner_name));
    }
    use_tree_local_name(tree).as_deref() == Some(owner_name)
}

fn use_tree_path_text(tree: &ast::UseTree) -> Option<String> {
    tree.path().map(|path| {
        let mut text: String = path
            .syntax()
            .text()
            .to_string()
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect();
        let leading_extern = path
            .syntax()
            .first_child_or_token()
            .is_some_and(|element| element.kind() == ra_ap_syntax::SyntaxKind::COLON2);
        if leading_extern && !text.starts_with("::") {
            text.insert_str(0, "::");
        }
        text
    })
}

fn use_tree_local_name(tree: &ast::UseTree) -> Option<String> {
    tree.rename()
        .and_then(|rename| rename.name())
        .map(|name| name.text().to_string())
        .or_else(|| {
            use_tree_path_text(tree)
                .as_ref()
                .and_then(|path| path.rsplit("::").next())
                .map(ToString::to_string)
        })
}

fn enclosing_item_container(function: &ast::Fn) -> Option<SyntaxNode> {
    function
        .syntax()
        .ancestors()
        .find(|node| ast::Module::can_cast(node.kind()) || ast::SourceFile::can_cast(node.kind()))
}

fn item_children(container: &SyntaxNode) -> Vec<SyntaxNode> {
    ast::Module::cast(container.clone())
        .and_then(|module| module.item_list())
        .map(|list| list.syntax().children().collect())
        .unwrap_or_else(|| container.children().collect())
}

fn use_binds_foreign_owner(
    item: &ast::Use,
    owner_name: &str,
    current_module: &str,
    owner_module: &str,
    index: &RustIndex,
) -> bool {
    item.use_tree().is_some_and(|tree| {
        use_tree_binds_foreign_owner(&tree, "", owner_name, current_module, owner_module, index)
    })
}

fn use_current_module(item: &ast::Use, file_module: &str) -> String {
    let nested: Vec<String> = item
        .syntax()
        .ancestors()
        .filter_map(ast::Module::cast)
        .filter_map(|module| module.name().map(|name| name.text().to_string()))
        .collect();
    let mut segments: Vec<String> = file_module
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(ToString::to_string)
        .collect();
    for name in nested.into_iter().rev() {
        segments.push(name);
    }
    segments.join("/")
}

fn use_tree_binds_foreign_owner(
    tree: &ast::UseTree,
    prefix: &str,
    owner_name: &str,
    current_module: &str,
    owner_module: &str,
    index: &RustIndex,
) -> bool {
    if tree.star_token().is_some() {
        return false;
    }
    let path_text = use_tree_path_text(tree);
    if let Some(list) = tree.use_tree_list() {
        let child_prefix = match (&path_text, prefix.is_empty()) {
            (Some(path), false) => format!("{prefix}::{path}"),
            (Some(path), true) => path.clone(),
            (None, _) => prefix.to_string(),
        };
        return list.use_trees().any(|nested| {
            use_tree_binds_foreign_owner(
                &nested,
                &child_prefix,
                owner_name,
                current_module,
                owner_module,
                index,
            )
        });
    }
    if use_tree_local_name(tree).as_deref() != Some(owner_name) {
        return false;
    }
    let full = match (&path_text, prefix.is_empty()) {
        (Some(path), false) => format!("{prefix}::{path}"),
        (Some(path), true) => path.clone(),
        (None, false) => prefix.to_string(),
        (None, true) => owner_name.to_string(),
    };
    !import_resolves_to_unique_owner(&full, current_module, owner_module, owner_name, index)
}

fn import_resolves_to_unique_owner(
    item_path: &str,
    current_module: &str,
    owner_module: &str,
    owner_name: &str,
    index: &RustIndex,
) -> bool {
    let Some(resolved_module) = resolve_use_module_path(item_path, current_module) else {
        return false;
    };
    if normalize_module_key(&resolved_module) != normalize_module_key(owner_module) {
        return false;
    }
    owner_name_is_unique_in_module(index, owner_module, owner_name)
}

fn resolve_use_module_path(item_path: &str, current_module: &str) -> Option<String> {
    // Rust 2018+ `::ident` selects the extern prelude, not a local module.
    if item_path.starts_with("::") {
        return None;
    }
    let mut segments: Vec<String> = current_module
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(ToString::to_string)
        .collect();
    let parts: Vec<&str> = item_path
        .split("::")
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        return None;
    }
    for part in &parts[..parts.len() - 1] {
        match *part {
            "super" => {
                segments.pop()?;
            }
            "self" => {}
            "crate" => segments.clear(),
            other => segments.push(other.to_string()),
        }
    }
    Some(segments.join("/"))
}

fn owner_name_is_unique_in_module(index: &RustIndex, module_path: &str, owner_name: &str) -> bool {
    let normalized = normalize_module_key(module_path);
    let mut seen = false;
    for function in &index.functions() {
        if function.source_role.is_evidence_role() {
            continue;
        }
        if function.name != owner_name {
            continue;
        }
        let Some(path) = module_path_for_index(index, &function.file) else {
            continue;
        };
        if normalize_module_key(&path) != normalized {
            continue;
        }
        if seen {
            return false;
        }
        seen = true;
    }
    seen
}

fn normalize_module_key(path: &str) -> String {
    path.replace('/', "::")
}

fn direct_owner_result_bindings(
    function: &ast::Fn,
    owner_name: &str,
    lines: &LineIndex,
) -> Vec<OwnerResultBinding> {
    let Some(statements) = function.body().and_then(|body| body.stmt_list()) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for statement in statements.statements() {
        let ast::Stmt::LetStmt(binding) = statement else {
            continue;
        };
        let Some(name) = plain_binding_name(&binding) else {
            continue;
        };
        let Some(name_range) = binding
            .pat()
            .and_then(|pattern| ast::IdentPat::cast(pattern.syntax().clone()))
            .and_then(|pattern| pattern.name())
            .map(|name| name.syntax().text_range())
        else {
            continue;
        };
        let Some(initializer) = binding.initializer() else {
            continue;
        };
        if !initializer_is_direct_owner_call(&initializer, owner_name) {
            continue;
        }
        let line = binding
            .let_token()
            .map(|token| lines.line(token.text_range().start()))
            .unwrap_or_else(|| lines.line(binding.syntax().text_range().start()));
        found.push(OwnerResultBinding {
            name,
            name_range,
            line,
            range_end: binding.syntax().text_range().end(),
            initializer_range: initializer.syntax().text_range(),
        });
    }
    found
}

fn plain_binding_name(statement: &ast::LetStmt) -> Option<String> {
    let pattern = ast::IdentPat::cast(statement.pat()?.syntax().clone())?;
    if pattern.ref_token().is_some() || pattern.at_token().is_some() || pattern.pat().is_some() {
        return None;
    }
    let name = pattern.name()?.text().to_string();
    valid_identifier(&name).then_some(name)
}

fn initializer_is_direct_owner_call(expression: &ast::Expr, owner_name: &str) -> bool {
    let Some(call) = ast::CallExpr::cast(expression.syntax().clone()) else {
        return false;
    };
    let Some(callee) = call.expr() else {
        return false;
    };
    direct_name(&callee).as_deref() == Some(owner_name)
}

fn direct_name(expression: &ast::Expr) -> Option<String> {
    let path = ast::PathExpr::cast(expression.syntax().clone())?.path()?;
    let text = path.syntax().text().to_string();
    valid_identifier(&text).then_some(text)
}

fn valid_identifier(text: &str) -> bool {
    let mut characters = text.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn binding_invalidated_before(
    function: &ast::Fn,
    binding: &OwnerResultBinding,
    assertion_line: usize,
    lines: &LineIndex,
) -> bool {
    let assertion_cutoff = function
        .syntax()
        .descendants()
        .find(|node| lines.line(node.text_range().start()) == assertion_line)
        .map(|node| node.text_range().start())
        .unwrap_or_else(|| function.syntax().text_range().end());
    let start = binding.range_end;
    if start >= assertion_cutoff {
        return true;
    }
    for node in function.syntax().descendants() {
        let range = node.text_range();
        if range.start() < start || range.start() >= assertion_cutoff {
            continue;
        }
        if let Some(pattern) = ast::IdentPat::cast(node.clone())
            && pattern
                .name()
                .is_some_and(|name| name.text() == binding.name)
        {
            return true;
        }
        if let Some(bin) = ast::BinExpr::cast(node.clone())
            && is_assignment_op(
                bin.op_token()
                    .as_ref()
                    .map(|token| token.text())
                    .unwrap_or(""),
            )
            && let Some(lhs) = bin.lhs()
            && binding_escape(&lhs, &binding.name)
        {
            return true;
        }
        if let Some(reference) = ast::RefExpr::cast(node)
            && reference.mut_token().is_some()
            && reference
                .expr()
                .is_some_and(|expr| binding_escape(&expr, &binding.name))
        {
            return true;
        }
    }
    false
}

fn is_assignment_op(token: &str) -> bool {
    matches!(
        token,
        "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>="
    )
}

fn binding_escape(expression: &ast::Expr, binding: &str) -> bool {
    if path_is_binding(expression, binding) || field_receiver_is_binding(expression, binding) {
        return true;
    }
    ast::ParenExpr::cast(expression.syntax().clone())
        .and_then(|paren| paren.expr())
        .is_some_and(|inner| binding_escape(&inner, binding))
}

fn path_is_binding(expression: &ast::Expr, binding: &str) -> bool {
    direct_name(expression).as_deref() == Some(binding)
}

fn field_receiver_is_binding(expression: &ast::Expr, binding: &str) -> bool {
    ast::FieldExpr::cast(expression.syntax().clone())
        .and_then(|field| field.expr())
        .is_some_and(|receiver| path_is_binding(&receiver, binding))
}

fn oracle_observes_binding_field(oracle_text: &str, binding: &str, field: &str) -> bool {
    let Some(source) = discriminating_oracle_source(oracle_text) else {
        return false;
    };
    let wrapped = format!("fn __ripr_owner_result_probe() {{ {source}\n}}");
    let Some(parse) = parse_clean_source_file(&wrapped) else {
        return false;
    };
    probe_reads_binding_field(&parse.tree(), binding, field)
}

fn discriminating_oracle_source(oracle_text: &str) -> Option<String> {
    let text = oracle_text.trim().trim_end_matches(';').trim();
    let (macro_name, open) = assertion_macro_open(text)?;
    let inner = super::delimited_contents_at(text, open)?;
    let mut args = super::split_top_level_commas(&inner).into_iter();
    match macro_name {
        "assert" => args.next().filter(|arg| !arg.is_empty()),
        "assert_eq" | "assert_ne" => {
            let left = args.next().filter(|arg| !arg.is_empty())?;
            let right = args.next().filter(|arg| !arg.is_empty())?;
            Some(format!("({left}, {right})"))
        }
        _ => None,
    }
}

fn assertion_macro_open(text: &str) -> Option<(&'static str, usize)> {
    ["assert_eq!", "assert_ne!", "assert!"]
        .into_iter()
        .find_map(|macro_name| {
            text.match_indices(macro_name).find_map(|(index, _)| {
                let prefix_ok = index == 0
                    || !text[..index]
                        .chars()
                        .next_back()
                        .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_');
                let suffix_start = index + macro_name.len();
                let open_offset = text[suffix_start..]
                    .char_indices()
                    .find_map(|(offset, ch)| (!ch.is_whitespace()).then_some((offset, ch)))?;
                if !prefix_ok || open_offset.1 != '(' {
                    return None;
                }
                let name = macro_name.trim_end_matches('!');
                Some((name, suffix_start + open_offset.0))
            })
        })
}

fn probe_reads_binding_field(probe: &SourceFile, binding: &str, field: &str) -> bool {
    probe
        .syntax()
        .descendants()
        .filter_map(ast::FieldExpr::cast)
        .any(|expr| {
            expr.name_ref().is_some_and(|name| name.text() == field)
                && expr
                    .expr()
                    .is_some_and(|receiver| path_is_binding(&receiver, binding))
                && !ident_pat_shadows_before(probe, binding, expr.syntax().text_range().start())
        })
}

fn ident_pat_shadows_before(probe: &SourceFile, binding: &str, at: ra_ap_syntax::TextSize) -> bool {
    probe
        .syntax()
        .descendants()
        .filter_map(ast::IdentPat::cast)
        .any(|pattern| {
            pattern.name().is_some_and(|name| name.text() == binding)
                && pattern.syntax().text_range().start() < at
        })
}

fn observation_from_oracle(
    kind: OracleKind,
    strength: OracleStrength,
) -> Option<OwnerResultObservation> {
    match (kind, strength) {
        (
            OracleKind::ExactValue | OracleKind::WholeObjectEquality,
            OracleStrength::Strong | OracleStrength::Medium,
        ) => Some(OwnerResultObservation::Strong),
        (OracleKind::RelationalCheck, OracleStrength::Strong | OracleStrength::Medium) => {
            Some(OwnerResultObservation::Strong)
        }
        (
            OracleKind::Snapshot | OracleKind::SmokeOnly,
            OracleStrength::Medium | OracleStrength::Strong,
        ) => Some(OwnerResultObservation::Strong),
        (
            OracleKind::RelationalCheck | OracleKind::Snapshot | OracleKind::SmokeOnly,
            OracleStrength::Weak | OracleStrength::Smoke,
        ) => Some(OwnerResultObservation::Weak),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The memoized lookup must agree with a full walk that takes the first
    /// matching `fn` in source order and refuses a second one.
    fn walk_unique_test_fn(parsed: &ParsedTestFile, name: &str, line: usize) -> Option<ast::Fn> {
        let mut matches = parsed
            .parse
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::Fn::cast)
            .filter(|function| {
                function
                    .name()
                    .is_some_and(|fn_name| fn_name.text() == name)
                    && function
                        .fn_token()
                        .is_some_and(|token| parsed.lines.line(token.text_range().start()) == line)
            });
        let function = matches.next()?;
        matches.next().is_none().then_some(function)
    }

    #[test]
    fn parsed_test_file_lookup_matches_a_full_tree_walk() -> Result<(), String> {
        let source = "\
#[test]
fn alone() { assert!(true); }
mod nested {
    #[test]
    fn alone() { assert!(true); }
}
fn twin() {} fn twin() {}
fn outer() { fn inner() {} }
";
        let parse = parse_clean_source_file(source).ok_or("fixture must parse")?;
        let parsed = ParsedTestFile::new(parse, source);
        let probes = [
            ("alone", 2),
            ("alone", 5),
            ("twin", 7),
            ("outer", 8),
            ("inner", 8),
            ("alone", 3),
            ("missing", 2),
        ];
        for (name, line) in probes {
            let memo = parsed
                .unique_test_fn(name, line)
                .map(|f| f.syntax().text_range());
            let walk = walk_unique_test_fn(&parsed, name, line).map(|f| f.syntax().text_range());
            if memo != walk {
                return Err(format!("{name}@{line}: memo {memo:?} != walk {walk:?}"));
            }
        }
        // Discriminating cases: the nested twin resolves to its own line, and
        // two `fn twin` on one line are ambiguous rather than first-wins.
        if parsed.unique_test_fn("alone", 5).is_none() {
            return Err("nested test fn must resolve".into());
        }
        if parsed.unique_test_fn("twin", 7).is_some() {
            return Err("two fns on one line must stay ambiguous".into());
        }
        Ok(())
    }
}
