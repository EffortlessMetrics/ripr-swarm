//! Builder override: a test that overwrites a changed field before reading it
//! (RIPR-SPEC-0040 builder override, #6613).
//!
//! A `return_value` probe on a struct literal (`RetryBuilder { attempts: 4,
//! delay_ms: 100 }`) or a `field_construction` probe on one of its fields
//! (`attempts: 4`) is confirmed by `reveal` when an assertion names one of the
//! changed tokens, typically a field read (`retry.attempts`). When the test
//! sets that same field on the owner's result before reading it
//! (`RetryBuilder::new().attempts(5).build()`), the changed initializer never
//! reaches the assertion: the read observes the test's own value, and the
//! shared field name is coincidence, not observation.
//!
//! Everything here is read from the syntax tree, and every decision fails
//! closed toward the existing credit: a field counts as overridden only when
//! the assertion is the one macro call on its `OracleFact` line, every mention
//! of the field in it is a field read, and each read's value is tied to the
//! owner call through its own method-call receiver chain or through `let`
//! bindings, with a setter whose result flows into the read value: a
//! setter-shaped call (`.field(arg)`, `.with_field(arg)`, `.set_field(arg)`)
//! on that receiver chain whose indexed body plainly stores its argument in
//! the field ([`setter_plainly_writes`]; a name alone proves nothing, since
//! `with_attempts(n)` may add or clamp), a reassignment `b = b.field(arg)`, or a field
//! assignment `b.field = value` after the binding's `let`. Setter calls inside
//! arguments, a setter whose result is discarded (`b.with_field(5);`), and
//! anything the tree cannot tie to the owner leave the token confirming, and
//! so does any other later statement that touches the binding
//! (`b.reset();`, `if c { b = .. }`, `let r = &mut b;`).

use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{
    AstNode, SourceFile, SyntaxKind, SyntaxNode, TextSize,
    ast::{self, HasArgList, HasName},
};

use super::super::rust_index::{OracleFact, RustIndex, TestSummary};

/// Upper bound on `let` hops from the assertion's receiver back to the owner
/// call (`retry` <- `builder.build()` <- `RetryBuilder::new()`).
const MAX_BINDING_HOPS: usize = 4;

/// The owner call a test value must come from: the owner's bare name and,
/// for an associated function, the type it is defined on
/// (`src/settings.rs::impl RetryBuilder::new` -> `RetryBuilder`, `new`).
#[derive(Clone, Copy, Debug)]
pub(super) struct OwnerCall<'a> {
    name: &'a str,
    impl_type: Option<&'a str>,
    /// Method names on the owner's type verified to store their argument in
    /// an overridden field ([`setter_plainly_writes`]). Empty until
    /// [`OwnerCall::with_setters`]: no method call then counts as a setter.
    setters: &'a [String],
}

impl<'a> OwnerCall<'a> {
    /// Reads the owner call from the owner symbol id. `None` when the id has
    /// no bare name.
    pub(super) fn from_symbol(symbol: &'a str) -> Option<Self> {
        let mut segments = symbol.rsplit("::");
        let name = segments.next().filter(|name| is_identifier(name))?;
        let impl_type = segments
            .next()
            .and_then(|segment| segment.strip_prefix("impl "))
            .map(|ty| {
                // `impl Trait for Type` names the type last; generics are
                // dropped from the type name.
                let ty = ty.rsplit(" for ").next().unwrap_or(ty).trim();
                ty.split('<').next().unwrap_or(ty).trim()
            });
        Some(Self {
            name,
            impl_type,
            setters: &[],
        })
    }

    /// The owner's bare name (`ship` for `impl Inventory::ship`).
    pub(super) fn name(&self) -> &'a str {
        self.name
    }

    /// The owner type, when the owner is an associated function.
    pub(super) fn impl_type(&self) -> Option<&'a str> {
        self.impl_type
    }

    /// The same owner with its verified setter method names.
    pub(super) fn with_setters(self, setters: &'a [String]) -> Self {
        Self { setters, ..self }
    }

    /// Whether `call` is a direct call of the owner: `RetryBuilder::new(..)`
    /// or `Self::new(..)` for an associated owner, a bare `owner(..)` for a
    /// free one.
    fn is_called_by(&self, call: &ast::CallExpr) -> bool {
        let Some(ast::Expr::PathExpr(callee)) = call.expr() else {
            return false;
        };
        let Some(path) = callee.path() else {
            return false;
        };
        let named = path
            .segment()
            .and_then(|segment| segment.name_ref())
            .is_some_and(|name| name.text() == self.name);
        if !named {
            return false;
        }
        let qualifier = path
            .qualifier()
            .and_then(|qualifier| qualifier.segment())
            .map(|segment| {
                segment
                    .name_ref()
                    .map(|name| name.text().to_string())
                    .unwrap_or_else(|| segment.syntax().text().to_string())
            });
        match (self.impl_type, qualifier.as_deref()) {
            (Some(ty), Some(qualifier)) => qualifier == ty || qualifier == "Self",
            (None, None) => true,
            _ => false,
        }
    }
}

/// Field names a struct-literal expression initializes, read from its
/// `RecordExpr` (`RetryBuilder { attempts: 4, delay_ms: 100 }` ->
/// `["attempts", "delay_ms"]`). Shorthand initializers count; a `..base`
/// spread does not. Empty for anything that is not one struct literal.
pub(super) fn struct_literal_fields(expression: &str) -> Vec<String> {
    let source = format!(
        "fn __ripr_struct_literal() {{ let _ = {}; }}",
        expression.trim().trim_end_matches(';')
    );
    let Some(parse) = parse_clean_source_file(&source) else {
        return Vec::new();
    };
    let Some(initializer) = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::LetStmt::cast)
        .and_then(|statement| statement.initializer())
    else {
        return Vec::new();
    };
    let ast::Expr::RecordExpr(record) = initializer else {
        return Vec::new();
    };
    record
        .record_expr_field_list()
        .into_iter()
        .flat_map(|list| list.fields())
        .filter_map(|field| field.field_name())
        .map(|name| name.text().to_string())
        .collect()
}

/// The subset of `fields` whose every mention in `assertion` is a field read
/// on a value the test overwrote that field on after obtaining it from the
/// `owner` call. Empty when the assertion cannot be located by its line in
/// `test`'s body, or when anything along the way cannot be read.
pub(super) fn overridden_fields_read(
    test: &TestSummary,
    assertion: &OracleFact,
    owner: OwnerCall<'_>,
    fields: &[String],
) -> Vec<String> {
    if fields.is_empty() {
        return Vec::new();
    }
    let body: &str = &test.body;
    let Some(parse) = parse_clean_source_file(body) else {
        return Vec::new();
    };
    let tree = parse.tree();
    let Some(assertion_call) = assertion_macro_at_line(&tree, body, test, assertion) else {
        return Vec::new();
    };
    let Some(arguments) = assertion_call.token_tree() else {
        return Vec::new();
    };
    let fragment_source = format!(
        "fn __ripr_assertion_arguments() {{ let _ = {}; }}",
        arguments.syntax().text()
    );
    let Some(fragment) = parse_clean_source_file(&fragment_source) else {
        return Vec::new();
    };
    let visible = visible_statements_before(assertion_call.syntax());
    fields
        .iter()
        .filter(|field| every_mention_is_overridden_read(&fragment.tree(), &visible, owner, field))
        .cloned()
        .collect()
}

/// The single macro call (`assert_eq!(..)`) starting on the assertion's line.
/// Two calls on the line, or none, leave the assertion unlocated.
fn assertion_macro_at_line(
    tree: &SourceFile,
    body: &str,
    test: &TestSummary,
    assertion: &OracleFact,
) -> Option<ast::MacroCall> {
    let relative_line = assertion.line.checked_sub(test.start_line)?;
    let mut calls = tree
        .syntax()
        .descendants()
        .filter_map(ast::MacroCall::cast)
        .filter(|call| {
            let start = macro_name_start(call);
            line_of(body, start) == Some(relative_line)
        });
    let call = calls.next()?;
    calls.next().is_none().then_some(call)
}

/// The macro call's own start, at its path (an attributed call spans its
/// attributes).
fn macro_name_start(call: &ast::MacroCall) -> TextSize {
    call.path()
        .map(|path| path.syntax().text_range().start())
        .unwrap_or_else(|| call.syntax().text_range().start())
}

/// Zero-based line of `offset` in `text`.
fn line_of(text: &str, offset: TextSize) -> Option<usize> {
    let offset = usize::from(offset);
    text.get(..offset)
        .map(|prefix| prefix.matches('\n').count())
}

fn every_mention_is_overridden_read(
    fragment: &SourceFile,
    visible: &[ast::Stmt],
    owner: OwnerCall<'_>,
    field: &str,
) -> bool {
    let mentions: Vec<_> = fragment
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.kind() == SyntaxKind::IDENT && token.text() == field)
        .collect();
    !mentions.is_empty()
        && mentions.iter().all(|token| {
            let Some(read) = token
                .parent()
                .and_then(ast::NameRef::cast)
                .and_then(|name| name.syntax().parent())
                .and_then(ast::FieldExpr::cast)
            else {
                return false;
            };
            read.expr().is_some_and(|receiver| {
                value_state(&receiver, visible, owner, field, MAX_BINDING_HOPS)
                    == ValueState::Overridden
            })
        })
}

/// What a test value carries in `field` relative to the owner's result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValueState {
    /// The value comes from the owner call and the test wrote `field` on it.
    Overridden,
    /// The value comes from the owner call and `field` was not written.
    Default,
    /// The value could not be tied to the owner call.
    Unknown,
}

/// Follows `expression`'s method-call receiver chain back to the owner call,
/// directly or through `let` bindings among `visible` (the statements that
/// run before it, nearest last). Only receivers are followed: a setter inside
/// an argument writes some other value.
fn value_state(
    expression: &ast::Expr,
    visible: &[ast::Stmt],
    owner: OwnerCall<'_>,
    field: &str,
    hops: usize,
) -> ValueState {
    let (root, setter_on_chain) = chain_root(expression, owner, field);
    let combine = |state: ValueState, written: bool| match state {
        ValueState::Unknown => ValueState::Unknown,
        _ if written => ValueState::Overridden,
        state => state,
    };
    match root {
        ast::Expr::CallExpr(call) if owner.is_called_by(&call) => {
            combine(ValueState::Default, setter_on_chain)
        }
        ast::Expr::PathExpr(path) => {
            let Some(binding) = single_identifier(&path) else {
                return ValueState::Unknown;
            };
            let Some(hops) = hops.checked_sub(1) else {
                return ValueState::Unknown;
            };
            let Some((index, statement)) = binding_let(visible, &binding) else {
                return ValueState::Unknown;
            };
            let Some(initializer) = statement.initializer() else {
                return ValueState::Unknown;
            };
            let bound = value_state(
                &initializer,
                &visible_statements_before(statement.syntax()),
                owner,
                field,
                hops,
            );
            match binding_writes(&visible[index + 1..], &binding, owner, field) {
                None => ValueState::Unknown,
                Some(written) => combine(bound, setter_on_chain || written),
            }
        }
        _ => ValueState::Unknown,
    }
}

/// The root of `expression`'s receiver chain (through method calls, parens
/// and `?`), and whether a setter of `field` is on the chain.
fn chain_root(expression: &ast::Expr, owner: OwnerCall<'_>, field: &str) -> (ast::Expr, bool) {
    let mut current = expression.clone();
    let mut setter = false;
    loop {
        let next = match &current {
            ast::Expr::MethodCallExpr(call) => {
                setter |= is_setter(call, owner, field);
                call.receiver()
            }
            ast::Expr::ParenExpr(paren) => paren.expr(),
            ast::Expr::TryExpr(try_expr) => try_expr.expr(),
            _ => None,
        };
        match next {
            Some(next) => current = next,
            None => return (current, setter),
        }
    }
}

/// The setter-shaped method names for `field`: `field`, `with_field`,
/// `set_field`.
pub(super) fn setter_names(field: &str) -> [String; 3] {
    [
        field.to_string(),
        format!("with_{field}"),
        format!("set_{field}"),
    ]
}

/// A setter-shaped call writing `field` with an argument (`.field(5)`,
/// `.with_field(5)`, `.set_field(5)`) whose method the owner verified. A
/// zero-argument `.field()` reads.
fn is_setter(call: &ast::MethodCallExpr, owner: OwnerCall<'_>, field: &str) -> bool {
    let named = call.name_ref().is_some_and(|name| {
        let name = name.text();
        setter_names(field).iter().any(|setter| setter == name)
            && owner.setters.iter().any(|setter| setter == name)
    });
    named
        && call
            .arg_list()
            .is_some_and(|arguments| arguments.args().next().is_some())
}

fn single_identifier(path: &ast::PathExpr) -> Option<String> {
    let path = path.path()?;
    if path.qualifier().is_some() {
        return None;
    }
    let name = path.segment()?.name_ref()?.text().to_string();
    is_identifier(&name).then_some(name)
}

/// The nearest `let [mut] binding = ..;` among `visible`, with its index.
/// A nearer `let` that binds the name through any other pattern
/// (destructuring, `ref`) shadows it unreadably, so nothing is returned.
fn binding_let(visible: &[ast::Stmt], binding: &str) -> Option<(usize, ast::LetStmt)> {
    visible
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, statement)| {
            let ast::Stmt::LetStmt(statement) = statement else {
                return None;
            };
            let pattern = statement.pat()?;
            let binds = pattern
                .syntax()
                .descendants()
                .filter_map(ast::IdentPat::cast)
                .any(|ident| ident.name().is_some_and(|name| name.text() == binding));
            binds.then(|| (index, statement.clone()))
        })
        .and_then(|(index, statement)| {
            let ast::Pat::IdentPat(ident) = statement.pat()? else {
                return None;
            };
            (ident.ref_token().is_none() && ident.pat().is_none() && statement.let_else().is_none())
                .then_some((index, statement))
        })
}

/// Whether `statements` (after the binding's `let`, before its use) write
/// `field` on `binding`: `binding = binding.field(5)` or `binding.field =
/// value` with a value not read from `binding`. `None` when `binding` is
/// reassigned from anything that is not its own setter chain, which leaves
/// what it holds unread.
fn binding_writes(
    statements: &[ast::Stmt],
    binding: &str,
    owner: OwnerCall<'_>,
    field: &str,
) -> Option<bool> {
    let mut written = false;
    for statement in statements {
        if !mentions_identifier(statement.syntax(), binding) {
            continue;
        }
        // A `let` reading the binding (`let retry = builder.build();`) leaves
        // it as it was unless it borrows it mutably.
        if let ast::Stmt::LetStmt(statement) = statement {
            if borrows_mutably(statement.syntax(), binding) {
                return None;
            }
            continue;
        }
        // Anything else touching the binding must be a recognized write.
        let ast::Stmt::ExprStmt(statement) = statement else {
            return None;
        };
        let Some(ast::Expr::BinExpr(assignment)) = statement.expr() else {
            return None;
        };
        if assignment.op_kind() != Some(ast::BinaryOp::Assignment { op: None }) {
            return None;
        }
        let (Some(target), Some(value)) = (assignment.lhs(), assignment.rhs()) else {
            return None;
        };
        match target {
            ast::Expr::PathExpr(path) if single_identifier(&path).as_deref() == Some(binding) => {
                let (root, setter) = chain_root(&value, owner, field);
                let rooted = matches!(&root, ast::Expr::PathExpr(path)
                    if single_identifier(path).as_deref() == Some(binding));
                if !rooted {
                    return None;
                }
                written |= setter;
            }
            ast::Expr::FieldExpr(target)
                if target.name_ref().is_some_and(|name| name.text() == field)
                    && target.expr().is_some_and(|receiver| {
                        matches!(&receiver, ast::Expr::PathExpr(path)
                            if single_identifier(path).as_deref() == Some(binding))
                    }) =>
            {
                written |= !mentions_identifier(value.syntax(), binding);
            }
            _ => return None,
        }
    }
    Some(written)
}

/// Whether `node` holds `&mut binding`.
fn borrows_mutably(node: &SyntaxNode, binding: &str) -> bool {
    node.descendants()
        .filter_map(ast::RefExpr::cast)
        .any(|reference| {
            reference.mut_token().is_some()
                && reference.expr().is_some_and(|expr| {
                    matches!(&expr, ast::Expr::PathExpr(path)
                        if single_identifier(path).as_deref() == Some(binding))
                })
        })
}

/// Whether the owner type's one indexed method named `setter` plainly stores
/// its single argument in `field`: it takes `self` (by value or reference)
/// and one identifier parameter, and every mention of `field` in its body is
/// `self.field = arg`, a record field `field: arg` (or shorthand), or the
/// parameter itself. A body that adds, clamps or reads the field
/// (`self.attempts += n`, `n.min(self.attempts)`), an ambiguous or missing
/// method, or a free owner leaves the call unverified.
pub(in crate::analysis) fn setter_plainly_writes(
    index: &RustIndex,
    impl_type: &str,
    setter: &str,
    field: &str,
) -> bool {
    let mut candidates = index.functions().iter().filter(|function| {
        function.name == setter
            && !function.source_role.is_evidence_role()
            && OwnerCall::from_symbol(&function.id.0)
                .is_some_and(|candidate| candidate.impl_type == Some(impl_type))
    });
    let (Some(function), None) = (candidates.next(), candidates.next()) else {
        return false;
    };
    let Some(parse) = parse_clean_source_file(&function.body) else {
        return false;
    };
    let Some(method) = parse.tree().syntax().descendants().find_map(ast::Fn::cast) else {
        return false;
    };
    let Some(parameters) = method.param_list() else {
        return false;
    };
    let mut rest = parameters.params();
    let (Some(_), Some(parameter), None) = (parameters.self_param(), rest.next(), rest.next())
    else {
        return false;
    };
    let Some(ast::Pat::IdentPat(parameter)) = parameter.pat() else {
        return false;
    };
    let Some(argument) = parameter.name().map(|name| name.text().to_string()) else {
        return false;
    };
    let Some(body) = method.body() else {
        return false;
    };
    let mentions: Vec<_> = body
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.kind() == SyntaxKind::IDENT && token.text() == field)
        .collect();
    let mut stores = false;
    for token in &mentions {
        let Some(name) = token.parent().and_then(ast::NameRef::cast) else {
            return false;
        };
        let Some(parent) = name.syntax().parent() else {
            return false;
        };
        if let Some(target) = ast::FieldExpr::cast(parent.clone()) {
            let on_self = target.expr().is_some_and(|receiver| {
                matches!(&receiver, ast::Expr::PathExpr(path)
                    if path.path().is_some_and(|path| path.syntax().text() == "self"))
            });
            let stored = target
                .syntax()
                .parent()
                .and_then(ast::BinExpr::cast)
                .filter(|assignment| {
                    assignment.op_kind() == Some(ast::BinaryOp::Assignment { op: None })
                        && assignment
                            .lhs()
                            .is_some_and(|lhs| lhs.syntax() == target.syntax())
                        // A top-level statement of the method body, not a
                        // conditional or looped store.
                        && assignment
                            .syntax()
                            .parent()
                            .filter(|statement| ast::ExprStmt::can_cast(statement.kind()))
                            .and_then(|statement| statement.parent())
                            .and_then(|list| list.parent())
                            .is_some_and(|block| &block == body.syntax())
                })
                .and_then(|assignment| assignment.rhs())
                .is_some_and(|value| is_path_named(&value, &argument));
            if !(on_self && stored) {
                return false;
            }
            stores = true;
        } else if let Some(record_field) = ast::RecordExprField::cast(parent.clone()) {
            if !record_field
                .expr()
                .is_some_and(|value| is_path_named(&value, &argument))
            {
                return false;
            }
            stores = true;
        } else if let Some(read) = parameter_read(&parent).filter(|_| field == argument) {
            // `self.attempts = attempts` names the parameter on the right,
            // checked at the field mention; shorthand `Self { attempts, .. }`
            // stores it.
            stores |= read
                .syntax()
                .parent()
                .and_then(ast::RecordExprField::cast)
                .is_some_and(|record_field| record_field.name_ref().is_none());
        } else {
            return false;
        }
    }
    stores
}

/// The body of the one non-test inherent method `name` on `impl_type`,
/// shared by getter credit and the pushed-literal callee-write check. Two
/// exclusions keep the body the one the call site runs: functions in an
/// evidence role (tests, fixtures) are skipped, and so are trait impls
/// (`impl Trait for Type`), since a trait method may dispatch to another
/// impl's body. `None` when no candidate remains, or several do.
pub(in crate::analysis) fn inherent_method_body<'a>(
    index: &'a RustIndex,
    impl_type: &str,
    name: &str,
) -> Option<&'a str> {
    let mut candidates = index.functions().iter().filter(|function| {
        function.name == name
            && !function.source_role.is_evidence_role()
            && !function.id.0.contains(" for ")
            && OwnerCall::from_symbol(&function.id.0)
                .is_some_and(|candidate| candidate.impl_type == Some(impl_type))
    });
    let (Some(function), None) = (candidates.next(), candidates.next()) else {
        return None;
    };
    Some(function.body.as_str())
}

/// RIPR-SPEC-0228 getter credit: whether `impl_type`'s method `getter`
/// takes only `&self` and its body is one tail expression returning the field
/// path `field` itself: `self.f`, `&self.f`, `&self.f[..]`, `self.f.clone()`
/// or `self.f.as_slice()`, in an inherent impl of `impl_type` resolved by
/// [`inherent_method_body`]. A body that reads another field, computes a value
/// from the field (`self.f.get(k).copied()`), or does anything else first,
/// and an ambiguous or missing method, give `false`: a method named after the
/// field is not evidence that it returns that field.
pub(in crate::analysis) fn getter_plainly_reads(
    index: &RustIndex,
    impl_type: &str,
    getter: &str,
    field: &str,
) -> bool {
    let Some(body) = inherent_method_body(index, impl_type, getter) else {
        return false;
    };
    let Some(parse) = parse_clean_source_file(body) else {
        return false;
    };
    let Some(method) = parse.tree().syntax().descendants().find_map(ast::Fn::cast) else {
        return false;
    };
    let takes_only_self_ref = method.param_list().is_some_and(|parameters| {
        parameters.params().next().is_none()
            && parameters.self_param().is_some_and(|receiver| {
                receiver.amp_token().is_some() && receiver.mut_token().is_none()
            })
    });
    let Some(body) = method.body().and_then(|body| body.stmt_list()) else {
        return false;
    };
    if !takes_only_self_ref || body.statements().next().is_some() {
        return false;
    }
    let Some(tail) = body.tail_expr() else {
        return false;
    };
    let text: String = tail
        .syntax()
        .text()
        .to_string()
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    let read = format!("self.{field}");
    [
        read.clone(),
        format!("&{read}"),
        format!("&{read}[..]"),
        format!("{read}.clone()"),
        format!("{read}.as_slice()"),
    ]
    .contains(&text)
}

/// RIPR-SPEC-0228 keyed getter credit: whether `impl_type`'s method
/// `getter` takes `&self` and one parameter `k` and its body is one tail
/// expression that looks `k` up in the field path `field`:
/// `self.f.get(k)` or `self.f.get(&k)`, optionally followed by `.copied()`
/// or `.cloned()`, then optionally by `.unwrap_or_default()` or
/// `.unwrap_or(<literal>)`. The method resolves as in
/// [`getter_plainly_reads`]; anything else (another field, another key,
/// arithmetic on the entry, a statement first) gives `None`. `Some` carries
/// what the getter returns for a key the map does not hold.
pub(in crate::analysis) fn getter_plainly_reads_key(
    index: &RustIndex,
    impl_type: &str,
    getter: &str,
    field: &str,
) -> Option<KeyedFallback> {
    keyed_getter_fallback(index, impl_type, getter, field)
}

/// What a keyed getter returns for a missing key: an assertion whose
/// expected value equals it passes whether or not the insert ran.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::analysis) enum KeyedFallback {
    /// `self.f.get(k)` with no default: `None`.
    Absent,
    /// `.unwrap_or(<literal>)`.
    Literal(String),
    /// `.unwrap_or_default()`: zero, `""` or `false`.
    TypeDefault,
}

impl KeyedFallback {
    /// Whether the `assert_eq!` operand `expected` is a value the getter
    /// cannot return for a missing key. Anything that is not a plain
    /// literal (or `Some(..)` against [`KeyedFallback::Absent`]) cannot be
    /// told apart and gives `false`.
    pub(in crate::analysis) fn differs_from(&self, expected: &str) -> bool {
        let expected = expected.trim();
        match self {
            Self::Absent => expected.starts_with("Some("),
            Self::Literal(fallback) => literal_value(expected)
                .zip(literal_value(fallback))
                .is_some_and(|(expected, fallback)| expected != fallback),
            Self::TypeDefault => literal_value(expected).is_some_and(|value| match value {
                LiteralValue::Number(number) => number != 0.0,
                LiteralValue::Text(text) => !text.is_empty(),
                LiteralValue::Bool(value) => value,
                LiteralValue::Char(_) => true,
            }),
        }
    }
}

#[derive(Debug, PartialEq)]
enum LiteralValue {
    Number(f64),
    Text(String),
    Char(String),
    Bool(bool),
}

/// The value of a plain literal ([`is_plain_literal`]), or of a negated
/// number. A radix-prefixed or exponent number (`0x7`, `1e3`: an unknown
/// suffix after the digits) or anything else gives `None`.
fn literal_value(text: &str) -> Option<LiteralValue> {
    let text = text.trim();
    if !(is_plain_literal(text) || text.strip_prefix('-').is_some_and(is_plain_literal)) {
        return None;
    }
    match text {
        "true" => return Some(LiteralValue::Bool(true)),
        "false" => return Some(LiteralValue::Bool(false)),
        _ => {}
    }
    let inner = || text.get(1..text.len() - 1).map(str::to_string);
    if text.starts_with('"') {
        return inner().map(LiteralValue::Text);
    }
    if text.starts_with('\'') {
        return inner().map(LiteralValue::Char);
    }
    let digits: String = text.chars().filter(|ch| *ch != '_').collect();
    let number_end = digits
        .char_indices()
        .find(|(at, ch)| !(ch.is_ascii_digit() || *ch == '.' || (*at == 0 && *ch == '-')))
        .map_or(digits.len(), |(at, _)| at);
    let (number, suffix) = digits.split_at(number_end);
    const SUFFIXES: [&str; 15] = [
        "", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize",
        "f32", "f64",
    ];
    if !SUFFIXES.contains(&suffix) {
        return None;
    }
    number.parse::<f64>().ok().map(LiteralValue::Number)
}

/// Whether a keyed read's `key` is a plain literal (`"BOLT"`, `7`), which
/// names the same value wherever it is spelled.
pub(in crate::analysis) fn is_literal_key(key: &str) -> bool {
    literal_value(key).is_some()
}

/// Whether `function` (its source text) binds or assigns `name` anywhere in
/// its body (`let sku = ..`, `sku = ..`, `if let Some(sku)`, `|sku|`), so a
/// use of `name` there may not be the parameter. Fails closed: a body that
/// does not parse counts as rebinding it.
pub(in crate::analysis) fn parameter_rebound(function: &str, name: &str) -> bool {
    let Some(body) = parse_clean_source_file(function)
        .and_then(|parse| parse.tree().syntax().descendants().find_map(ast::Fn::cast))
        .and_then(|method| method.body())
    else {
        return true;
    };
    body.syntax().descendants().any(|node| {
        ast::IdentPat::cast(node.clone())
            .and_then(|pat| pat.name())
            .is_some_and(|bound| bound.text() == name)
            || ast::BinExpr::cast(node).is_some_and(|expr| {
                matches!(expr.op_kind(), Some(ast::BinaryOp::Assignment { .. }))
                    && expr
                        .lhs()
                        .is_some_and(|lhs| lhs.syntax().text().to_string().trim() == name)
            })
    })
}

/// Whether `function` (its source text) declares a return type. Fails
/// closed: a body that does not parse counts as returning a value.
pub(in crate::analysis) fn function_returns_value(function: &str) -> bool {
    parse_clean_source_file(function)
        .and_then(|parse| parse.tree().syntax().descendants().find_map(ast::Fn::cast))
        .is_none_or(|method| method.ret_type().is_some())
}

fn keyed_getter_fallback(
    index: &RustIndex,
    impl_type: &str,
    getter: &str,
    field: &str,
) -> Option<KeyedFallback> {
    let body = inherent_method_body(index, impl_type, getter)?;
    let parse = parse_clean_source_file(body)?;
    let method = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    let parameters = method.param_list()?;
    let shared_self = parameters
        .self_param()
        .is_some_and(|receiver| receiver.amp_token().is_some() && receiver.mut_token().is_none());
    let mut params = parameters.params();
    let key = match (params.next(), params.next()) {
        (Some(param), None) => match param.pat() {
            Some(ast::Pat::IdentPat(pat))
                if pat.ref_token().is_none() && pat.mut_token().is_none() =>
            {
                pat.name().map(|name| name.text().to_string())
            }
            _ => None,
        },
        _ => None,
    };
    let (true, Some(key)) = (shared_self, key) else {
        return None;
    };
    let body = method.body().and_then(|body| body.stmt_list())?;
    if body.statements().next().is_some() {
        return None;
    }
    let tail = body.tail_expr()?;
    let text: String = tail
        .syntax()
        .text()
        .to_string()
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    let rest = [
        format!("self.{field}.get({key})"),
        format!("self.{field}.get(&{key})"),
    ]
    .iter()
    .find_map(|lookup| text.strip_prefix(lookup.as_str()))?;
    let rest = rest
        .strip_prefix(".copied()")
        .or_else(|| rest.strip_prefix(".cloned()"))
        .unwrap_or(rest);
    if rest.is_empty() {
        return Some(KeyedFallback::Absent);
    }
    if rest == ".unwrap_or_default()" {
        return Some(KeyedFallback::TypeDefault);
    }
    if !rest.starts_with(".unwrap_or(") {
        return None;
    }
    // The whitespace-free `text` only matches the shape; the default comes
    // from the syntax tree so a string default keeps its inner spaces.
    let call = ast::MethodCallExpr::cast(tail.syntax().clone())?;
    if call.name_ref()?.text() != "unwrap_or" {
        return None;
    }
    let mut arguments = call.arg_list()?.args();
    let default = match (arguments.next(), arguments.next()) {
        (Some(default), None) => default.syntax().text().to_string(),
        _ => return None,
    };
    is_plain_literal(&default).then_some(KeyedFallback::Literal(default))
}

/// A number, `true`/`false`, or a string or char literal without escapes:
/// a default that cannot read the receiver or the key.
fn is_plain_literal(text: &str) -> bool {
    let quoted = |quote: char| {
        text.len() >= 2
            && text.starts_with(quote)
            && text.ends_with(quote)
            && text
                .get(1..text.len() - 1)
                .is_some_and(|inner| !inner.contains([quote, '\\']))
    };
    matches!(text, "true" | "false")
        || quoted('"')
        || quoted('\'')
        || (text.starts_with(|ch: char| ch.is_ascii_digit())
            && text
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'))
}

fn is_path_named(expression: &ast::Expr, name: &str) -> bool {
    matches!(expression, ast::Expr::PathExpr(path)
        if single_identifier(path).as_deref() == Some(name))
}

/// The plain variable read (`attempts`) whose single path segment is a
/// `NameRef`'s parent.
fn parameter_read(parent: &SyntaxNode) -> Option<ast::PathExpr> {
    ast::PathSegment::cast(parent.clone())
        .and_then(|segment| segment.syntax().parent())
        .and_then(ast::Path::cast)
        .filter(|path| path.qualifier().is_none())
        .and_then(|path| path.syntax().parent())
        .and_then(ast::PathExpr::cast)
}

fn mentions_identifier(node: &SyntaxNode, name: &str) -> bool {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .any(|token| token.kind() == SyntaxKind::IDENT && token.text() == name)
}

/// The statements that run before `node` in straight-line order: for each
/// enclosing block, its statements that end before the branch holding
/// `node`, outermost block first. Stops at a closure or nested function, whose
/// body runs at another time.
fn visible_statements_before(node: &SyntaxNode) -> Vec<ast::Stmt> {
    let mut levels = Vec::new();
    let mut child = node.clone();
    while let Some(parent) = child.parent() {
        if ast::ClosureExpr::can_cast(parent.kind()) || ast::Fn::can_cast(parent.kind()) {
            break;
        }
        if ast::StmtList::can_cast(parent.kind()) {
            let start = child.text_range().start();
            let level: Vec<ast::Stmt> = parent
                .children()
                .filter(|statement| statement.text_range().end() <= start)
                .filter_map(ast::Stmt::cast)
                .collect();
            levels.push(level);
        }
        child = parent;
    }
    levels.into_iter().rev().flatten().collect()
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{OracleKind, OracleStrength};
    use std::path::PathBuf;

    const OWNER: &str = "src/settings.rs::impl RetryBuilder::new";

    fn fields() -> Vec<String> {
        struct_literal_fields("RetryBuilder { attempts: 4, delay_ms: 100 }")
    }

    /// A test whose `fn` keyword sits on line 10, with the assertion on the
    /// body line holding `assert`.
    fn overridden(statements: &str) -> Vec<String> {
        let body = format!("fn case() {{\n{statements}\n}}");
        let line = body
            .lines()
            .position(|line| line.contains("assert"))
            .map_or(0, |index| index + 10);
        let assertion_text = body
            .lines()
            .find(|line| line.contains("assert"))
            .unwrap_or_default()
            .trim()
            .to_string();
        let test = TestSummary {
            name: "case".to_string(),
            file: PathBuf::from("src/settings.rs"),
            start_line: 10,
            end_line: 10 + body.lines().count(),
            body: body.as_str().into(),
            calls: Vec::new(),
            assertions: Vec::new(),
            literals: Vec::new(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        };
        let assertion = OracleFact {
            line,
            text: assertion_text,
            kind: OracleKind::ExactValue,
            strength: OracleStrength::Strong,
            observed_tokens: Vec::new(),
            ok_value_observed: None,
        };
        let setters = setter_names("attempts");
        OwnerCall::from_symbol(OWNER).map_or_else(Vec::new, |owner| {
            overridden_fields_read(&test, &assertion, owner.with_setters(&setters), &fields())
        })
    }

    fn attempts() -> Vec<String> {
        vec!["attempts".to_string()]
    }

    #[test]
    fn struct_literal_fields_reads_the_record_expression() {
        assert_eq!(
            fields(),
            vec!["attempts".to_string(), "delay_ms".to_string()]
        );
        assert_eq!(
            struct_literal_fields("Self { attempts, delay: Duration::from_millis(1), ..base }"),
            vec!["attempts".to_string(), "delay".to_string()]
        );
        assert!(struct_literal_fields("attempts + 1").is_empty());
        assert!(struct_literal_fields("if x { 1 } else { 2 }").is_empty());
        assert!(struct_literal_fields("Some(Retry { attempts: 1 })").is_empty());
    }

    #[test]
    fn owner_call_reads_the_impl_type() {
        let owner = OwnerCall::from_symbol(OWNER).map(|owner| (owner.name, owner.impl_type));
        assert_eq!(owner, Some(("new", Some("RetryBuilder"))));
        let free = OwnerCall::from_symbol("src/lib.rs::defaults")
            .map(|owner| (owner.name, owner.impl_type));
        assert_eq!(free, Some(("defaults", None)));
    }

    fn settings_index(setter: &str) -> RustIndex {
        let source = format!(
            "pub struct RetryBuilder {{ attempts: u32, delay_ms: u64 }}\n\
             impl RetryBuilder {{\n\
                 pub fn new() -> Self {{ RetryBuilder {{ attempts: 3, delay_ms: 100 }} }}\n\
                 {setter}\n\
             }}\n"
        );
        let path = PathBuf::from("src/settings.rs");
        let facts = crate::analysis::rust_index::summarize_file(path.clone(), source);
        let mut index = RustIndex::default();
        index.extend_functions(facts.functions.iter().cloned());
        index.insert_file_only(path, facts);
        index
    }

    #[test]
    fn setter_counts_only_when_its_body_plainly_stores_the_argument() {
        for setter in [
            "pub fn attempts(mut self, attempts: u32) -> Self { self.attempts = attempts; self }",
            "pub fn attempts(mut self, n: u32) -> Self { self.attempts = n; self }",
            "pub fn attempts(&mut self, n: u32) -> &mut Self { self.attempts = n; self }",
            "pub fn attempts(self, attempts: u32) -> Self { Self { attempts, ..self } }",
            "pub fn attempts(self, n: u32) -> Self { RetryBuilder { attempts: n, ..self } }",
        ] {
            let index = settings_index(setter);
            assert!(
                setter_plainly_writes(&index, "RetryBuilder", "attempts", "attempts"),
                "{setter}"
            );
        }
        for setter in [
            // Adds, clamps, conditionally stores, or stores something else.
            "pub fn attempts(mut self, n: u32) -> Self { self.attempts += n; self }",
            "pub fn attempts(mut self, n: u32) -> Self { self.attempts = n.min(self.attempts); self }",
            "pub fn attempts(mut self, n: u32) -> Self { if n > 0 { self.attempts = n; } self }",
            "pub fn attempts(mut self, n: u32) -> Self { self.delay_ms = u64::from(n); self }",
            "pub fn attempts(self, n: u32) -> Self { self }",
            // Two parameters, or no `self`.
            "pub fn attempts(mut self, n: u32, m: u32) -> Self { self.attempts = n; self }",
        ] {
            let index = settings_index(setter);
            assert!(
                !setter_plainly_writes(&index, "RetryBuilder", "attempts", "attempts"),
                "{setter}"
            );
        }
        // Another type's method of the same name proves nothing.
        let index =
            settings_index("pub fn attempts(mut self, n: u32) -> Self { self.attempts = n; self }");
        assert!(!setter_plainly_writes(
            &index, "Retry", "attempts", "attempts"
        ));
    }

    /// RIPR-SPEC-0228 examples 12 and 13: a getter reads a field only when
    /// its body returns that field itself.
    #[test]
    fn getter_counts_only_when_its_body_returns_the_field() {
        for getter in [
            "pub fn entries(&self) -> &[u32] { &self.attempts }",
            "pub fn entries(&self) -> u32 { self.attempts }",
            "pub fn entries(&self) -> &[u32] { &self.attempts[..] }",
            "pub fn entries(&self) -> Vec<u32> { self.attempts.clone() }",
            "pub fn entries(&self) -> &[u32] { self.attempts.as_slice() }",
        ] {
            let index = settings_index(getter);
            assert!(
                getter_plainly_reads(&index, "RetryBuilder", "entries", "attempts"),
                "{getter}"
            );
        }
        for getter in [
            // Another field, a value computed from the field, a statement
            // first, an argument, or a `&mut self` receiver.
            "pub fn entries(&self) -> u64 { self.delay_ms }",
            "pub fn entries(&self) -> u32 { self.attempts + 1 }",
            "pub fn entries(&self) -> Option<u32> { self.attempts.checked_sub(1) }",
            "pub fn entries(&self) -> u32 { let n = self.attempts; n }",
            "pub fn entries(&self, n: u32) -> u32 { self.attempts }",
            "pub fn entries(&mut self) -> u32 { self.attempts }",
        ] {
            let index = settings_index(getter);
            assert!(
                !getter_plainly_reads(&index, "RetryBuilder", "entries", "attempts"),
                "{getter}"
            );
        }
        // A method named after the field that returns another field.
        let index = settings_index("pub fn attempts(&self) -> u64 { self.delay_ms }");
        assert!(!getter_plainly_reads(
            &index,
            "RetryBuilder",
            "attempts",
            "attempts"
        ));
        // Another type's method of the same name proves nothing.
        let index = settings_index("pub fn entries(&self) -> u32 { self.attempts }");
        assert!(!getter_plainly_reads(
            &index, "Retry", "entries", "attempts"
        ));
        // A trait method is not an inherent getter.
        let path = PathBuf::from("src/settings.rs");
        let facts = crate::analysis::rust_index::summarize_file(
            path.clone(),
            "pub struct RetryBuilder { attempts: u32 }\n\
             pub trait Entries { fn entries(&self) -> u32; }\n\
             impl Entries for RetryBuilder { fn entries(&self) -> u32 { self.attempts } }\n"
                .to_string(),
        );
        let mut index = RustIndex::default();
        index.extend_functions(facts.functions.iter().cloned());
        index.insert_file_only(path, facts);
        assert!(!getter_plainly_reads(
            &index,
            "RetryBuilder",
            "entries",
            "attempts"
        ));
    }

    /// RIPR-SPEC-0228 keyed reads: a getter reads one entry of a field map
    /// only when its body looks its own argument up in that field.
    #[test]
    fn keyed_getter_counts_only_when_it_looks_its_argument_up_in_the_field() {
        for (getter, fallback) in [
            (
                "pub fn entry(&self, k: &str) -> Option<&u32> { self.attempts.get(k) }",
                KeyedFallback::Absent,
            ),
            (
                "pub fn entry(&self, k: &str) -> u32 { self.attempts.get(k).copied().unwrap_or(0) }",
                KeyedFallback::Literal("0".to_string()),
            ),
            (
                "pub fn entry(&self, k: String) -> Option<u32> { self.attempts.get(&k).cloned() }",
                KeyedFallback::Absent,
            ),
            (
                "pub fn entry(&self, k: &str) -> u32 { self.attempts.get(k).copied().unwrap_or_default() }",
                KeyedFallback::TypeDefault,
            ),
            (
                "pub fn entry(&self, k: &str) -> &str { self.attempts.get(k).copied().unwrap_or(\"none\") }",
                KeyedFallback::Literal("\"none\"".to_string()),
            ),
            (
                "pub fn entry(&self, k @ _: &str) -> Option<&u32> { self.attempts.get(k) }",
                KeyedFallback::Absent,
            ),
            (
                "pub fn entry(&self, k: &str) -> &str { self.attempts.get(k).copied().unwrap_or(\"n/a x\") }",
                KeyedFallback::Literal("\"n/a x\"".to_string()),
            ),
        ] {
            let index = settings_index(getter);
            assert_eq!(
                getter_plainly_reads_key(&index, "RetryBuilder", "entry", "attempts"),
                Some(fallback),
                "{getter}"
            );
        }
        for getter in [
            // Another field, another key, arithmetic on the entry, a default
            // that reads state, a statement first, two arguments, no
            // argument, a destructured or `mut` argument, `&mut self`.
            "pub fn entry(&self, k: &str) -> Option<&u64> { self.delay_ms.get(k) }",
            "pub fn entry(&self, k: &str) -> Option<&u32> { self.attempts.get(\"x\") }",
            "pub fn entry(&self, k: &str) -> u32 { self.attempts.get(k).copied().unwrap_or(0) + 1 }",
            "pub fn entry(&self, k: &str) -> u32 { self.attempts.get(k).copied().unwrap_or(self.base) }",
            "pub fn entry(&self, k: &str) -> u32 { self.attempts.get(k).copied().map(|n| n + 1).unwrap_or(0) }",
            "pub fn entry(&self, k: &str) -> Option<&u32> { let k = k.trim(); self.attempts.get(k) }",
            "pub fn entry(&self, k: &str, j: &str) -> Option<&u32> { self.attempts.get(k) }",
            "pub fn entry(&self) -> Option<&u32> { self.attempts.get(\"x\") }",
            "pub fn entry(&self, (k, _): (&str, u8)) -> Option<&u32> { self.attempts.get(k) }",
            "pub fn entry(&self, mut k: &str) -> Option<&u32> { self.attempts.get(k) }",
            "pub fn entry(&mut self, k: &str) -> Option<&u32> { self.attempts.get(k) }",
            "pub fn entry(&self, k: &str) -> bool { self.attempts.contains_key(k) }",
        ] {
            let index = settings_index(getter);
            assert_eq!(
                getter_plainly_reads_key(&index, "RetryBuilder", "entry", "attempts"),
                None,
                "{getter}"
            );
        }
        // Another type's method of the same name proves nothing.
        let index =
            settings_index("pub fn entry(&self, k: &str) -> Option<&u32> { self.attempts.get(k) }");
        assert_eq!(
            getter_plainly_reads_key(&index, "Retry", "entry", "attempts"),
            None
        );
    }

    /// RIPR-SPEC-0228 keyed reads: an expected value the getter also
    /// returns for a missing key cannot show the insert ran.
    #[test]
    fn keyed_fallback_differs_only_from_a_distinct_literal() {
        let zero = KeyedFallback::Literal("0".to_string());
        assert!(zero.differs_from("7"));
        assert!(zero.differs_from("7u32"));
        assert!(zero.differs_from("-1"));
        for same_or_unknown in ["0", "0u32", "0_0", "0.0", "expected", "0x7", "1e3", "7 + 0"] {
            assert!(!zero.differs_from(same_or_unknown), "{same_or_unknown}");
        }
        let none = KeyedFallback::Literal("\"none\"".to_string());
        assert!(none.differs_from("\"bolt\""));
        assert!(!none.differs_from("\"none\""));
        assert!(KeyedFallback::Absent.differs_from("Some(&7)"));
        assert!(!KeyedFallback::Absent.differs_from("None"));
        assert!(!KeyedFallback::Absent.differs_from("expected"));
        let default = KeyedFallback::TypeDefault;
        for distinct in ["5", "\"x\"", "true", "'a'", "0.5"] {
            assert!(default.differs_from(distinct), "{distinct}");
        }
        for same_or_unknown in ["0", "0usize", "\"\"", "false", "count", "-0"] {
            assert!(!default.differs_from(same_or_unknown), "{same_or_unknown}");
        }
    }

    #[test]
    fn parameter_rebound_finds_shadows_and_assignments() {
        let rebound = |body: &str| {
            parameter_rebound(
                &format!("pub fn put(&mut self, sku: &str, qty: u32) {{ {body} }}"),
                "sku",
            )
        };
        assert!(!rebound("self.m.insert(sku.to_string(), qty);"));
        assert!(!rebound(
            "let other = sku; self.m.insert(sku.to_string(), qty);"
        ));
        for shadowing in [
            "let sku = \"LAST\"; self.m.insert(sku.to_string(), qty);",
            "if let Some(sku) = self.alias.get(sku) { self.m.insert(sku.to_string(), qty); }",
            "self.items.iter().for_each(|sku| { self.m.insert(sku.to_string(), qty); });",
            "let mut s = sku; s = \"x\"; let sku = s; self.m.insert(sku.to_string(), qty);",
        ] {
            assert!(rebound(shadowing), "{shadowing}");
        }
        assert!(parameter_rebound(
            "pub fn put(&mut self, mut sku: &str) { sku = \"x\"; self.m.insert(sku.to_string(), 1); }",
            "sku"
        ));
        assert!(parameter_rebound("not rust {", "sku"));
    }

    #[test]
    fn function_returns_value_reads_the_signature() {
        assert!(function_returns_value(
            "pub fn place(&mut self, q: u32) -> Result<(), String> { Ok(()) }"
        ));
        assert!(!function_returns_value(
            "pub fn restock(&mut self, sku: &str, qty: u32) { }"
        ));
        assert!(!function_returns_value(
            "pub fn f<F: Fn() -> u8>(&mut self, f: F) { }"
        ));
        assert!(function_returns_value("not rust {"));
    }

    #[test]
    fn unverified_setter_name_leaves_the_token_confirming() {
        let body = "fn case() {\n    let retry = RetryBuilder::new().attempts(5).build();\n    assert_eq!(retry.attempts, 5);\n}";
        let test = TestSummary {
            name: "case".to_string(),
            file: PathBuf::from("src/settings.rs"),
            start_line: 10,
            end_line: 13,
            body: body.into(),
            calls: Vec::new(),
            assertions: Vec::new(),
            literals: Vec::new(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        };
        let assertion = OracleFact {
            line: 12,
            text: "assert_eq!(retry.attempts, 5);".to_string(),
            kind: OracleKind::ExactValue,
            strength: OracleStrength::Strong,
            observed_tokens: Vec::new(),
            ok_value_observed: None,
        };
        let read = OwnerCall::from_symbol(OWNER)
            .map(|owner| overridden_fields_read(&test, &assertion, owner, &fields()));
        assert_eq!(read, Some(Vec::new()));
    }

    #[test]
    fn setter_chained_into_the_binding_overrides_the_read_field() {
        assert_eq!(
            overridden(
                "    let retry = RetryBuilder::new().attempts(5).build();\n    assert_eq!(retry.attempts, 5);"
            ),
            attempts()
        );
        // The chain inside the assertion itself, and `with_`/`set_` setters.
        assert_eq!(
            overridden("    assert_eq!(RetryBuilder::new().with_attempts(5).build().attempts, 5);"),
            attempts()
        );
        assert_eq!(
            overridden(
                "    let retry = Self::new().set_attempts(5).build();\n    assert_eq!(retry.attempts, 5);"
            ),
            attempts()
        );
    }

    #[test]
    fn setter_reaching_the_value_through_let_hops_overrides() {
        // Setter on an intermediate binding's chain.
        assert_eq!(
            overridden(
                "    let builder = RetryBuilder::new();\n    let retry = builder.attempts(5).build();\n    assert_eq!(retry.attempts, 5);"
            ),
            attempts()
        );
        // Reassigned through its own setter.
        assert_eq!(
            overridden(
                "    let mut builder = RetryBuilder::new();\n    builder = builder.attempts(5);\n    let retry = builder.build();\n    assert_eq!(retry.attempts, 5);"
            ),
            attempts()
        );
        // A field assignment after the binding's `let`.
        assert_eq!(
            overridden(
                "    let mut builder = RetryBuilder::new();\n    builder.attempts = 5;\n    let retry = builder.build();\n    assert_eq!(retry.attempts, 5);"
            ),
            attempts()
        );
    }

    #[test]
    fn default_read_without_setter_is_not_overridden() {
        assert!(
            overridden(
                "    let retry = RetryBuilder::new().build();\n    assert_eq!(retry.attempts, 3);"
            )
            .is_empty()
        );
    }

    #[test]
    fn setter_on_another_field_leaves_the_read_field_observed() {
        assert!(
            overridden(
                "    let retry = RetryBuilder::new().delay_ms(5).build();\n    assert_eq!(retry.attempts, 3);"
            )
            .is_empty()
        );
    }

    #[test]
    fn discarded_setter_result_is_not_an_override() {
        // A by-value setter whose new builder is dropped leaves `builder`
        // defaulted; a `&mut self` setter cannot be told apart here, so the
        // statement keeps the credit either way.
        assert!(
            overridden(
                "    let builder = RetryBuilder::new();\n    builder.with_attempts(5);\n    let retry = builder.build();\n    assert_eq!(retry.attempts, 3);"
            )
            .is_empty()
        );
    }

    #[test]
    fn a_later_unrecognized_touch_of_the_binding_leaves_it_unread() {
        // The field write is real, but a later mutation, a conditional
        // reassignment or a mutable borrow may restore the default.
        for later in [
            "    builder.reset();",
            "    if reset { builder = RetryBuilder::new(); }",
            "    let alias = &mut builder;\n    alias.attempts = 3;",
            "    builder.attempts += 1;",
        ] {
            let statements = format!(
                "    let mut builder = RetryBuilder::new();\n    builder.attempts = 5;\n{later}\n    assert_eq!(builder.attempts, 5);"
            );
            assert!(overridden(&statements).is_empty(), "{later}");
        }
        // A plain read of the binding keeps the override.
        assert_eq!(
            overridden(
                "    let mut builder = RetryBuilder::new();\n    builder.attempts = 5;\n    let label = format!(\"{}\", builder.delay_ms);\n    assert_eq!(builder.attempts, 5);"
            ),
            attempts()
        );
    }

    #[test]
    fn setter_inside_an_argument_is_not_on_the_chain() {
        assert!(
            overridden(
                "    let retry = RetryBuilder::new().with_other(other.attempts(5)).build();\n    assert_eq!(retry.attempts, 3);"
            )
            .is_empty()
        );
    }

    #[test]
    fn writes_before_a_shadowing_let_do_not_count() {
        // The setter applies to the first `builder`; the read comes from the
        // second, defaulted one.
        assert!(
            overridden(
                "    let mut builder = RetryBuilder::new();\n    builder.attempts = 5;\n    let builder = RetryBuilder::new();\n    let retry = builder.build();\n    assert_eq!(retry.attempts, 3);"
            )
            .is_empty()
        );
    }

    #[test]
    fn unrelated_or_unreadable_values_keep_the_credit() {
        // A setter on a value from another type's `new`.
        assert!(
            overridden(
                "    let retry = Other::new().attempts(5).build();\n    assert_eq!(retry.attempts, 5);"
            )
            .is_empty()
        );
        // A binding the test never `let`-binds (a parameter, a static).
        assert!(overridden("    assert_eq!(retry.attempts, 5);").is_empty());
        // A field read on a setter result but also a getter-shaped mention.
        assert!(
            overridden(
                "    let retry = RetryBuilder::new().attempts(5).build();\n    assert_eq!(retry.attempts, retry.attempts());"
            )
            .is_empty()
        );
        // The field name only in a string literal or comment of the body.
        assert!(
            overridden(
                "    // retry.attempts(5)\n    let retry = RetryBuilder::new().build();\n    assert_eq!(retry.attempts, 3, \".attempts(5)\");"
            )
            .is_empty()
        );
        // A write inside a branch is not straight-line.
        assert!(
            overridden(
                "    let mut builder = RetryBuilder::new();\n    if flag { builder.attempts = 5; }\n    let retry = builder.build();\n    assert_eq!(retry.attempts, 3);"
            )
            .is_empty()
        );
    }

    #[test]
    fn an_unlocatable_assertion_keeps_the_credit() {
        // Two assertions on the oracle's line: which one the fact is cannot
        // be read, so nothing is overridden.
        assert!(
            overridden(
                "    let retry = RetryBuilder::new().attempts(5).build();\n    assert_eq!(retry.attempts, 5); assert!(true);"
            )
            .is_empty()
        );
    }
}
