//! #14206: an assertion that calls the changed function but cannot observe
//! the call's value.
//!
//! A predicate probe takes the strong oracle from any exact assertion whose
//! line holds a call of the owner. That is right when the call's value
//! reaches the compared value (`assert_eq!(over(10), true)`,
//! `assert_eq!(!over(10), false)`, `assert_eq!(if over(10) { 1 } else { 2 },
//! 1)`), and wrong when the test discards or masks it first:
//! `assert_eq!(Some(over(10)).is_some(), true)` passes for either version of
//! `over`, and so do a tuple projection of another element, a block statement,
//! an argument a helper may ignore, and a lazy arm or closure that may never
//! run.
//!
//! This module answers one question about one assertion: does the owner
//! call's value reach a compared operand? It reads the compared operands only
//! (never the message arguments) and follows a `let` the operand names when
//! the test body binds that name exactly once.
//!
//! The reading is fail-closed in one direction only. An assertion that names
//! no owner call is [`OwnerValueFlow::Absent`] and keeps today's reading: the
//! gate has nothing to say about a test that reaches the owner some other
//! way. An assertion that names one is credited only on a shape listed here;
//! every other shape is [`OwnerValueFlow::Masked`]. Static evidence only: a
//! `Reaches` reading says the syntax carries the value, not that the test
//! would fail.

use super::super::rust_index::TestSummary;
use super::reveal::contains_as_whole_word;
use crate::analysis::extract::{assertion_oracle_text, mask_comments_and_strings};
use crate::analysis::syntax::parse_clean_source_file_memoized;
use ra_ap_syntax::{AstNode, ast, ast::HasArgList, ast::HasGenericArgs, ast::HasName};

/// How the owner call's value relates to an assertion's compared operands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::analysis) enum OwnerValueFlow {
    /// No compared operand names an owner call, directly or through a
    /// single-binding `let`.
    Absent,
    /// An operand names an owner call, and no listed shape carries its value
    /// to the compared value.
    Masked,
    /// The owner call's value reaches a compared operand.
    Reaches,
}

/// Let chains longer than this are not followed.
const LET_DEPTH: usize = 4;

/// Methods that take a test-built container's payload out, so the result is
/// the owner's value again.
const UNWRAPPING_METHODS: &[&str] = &[
    "unwrap",
    "expect",
    "unwrap_or",
    "unwrap_or_default",
    "unwrap_unchecked",
    "into_inner",
];

/// Methods that keep a test-built container's payload in the result.
const PAYLOAD_KEEPING_METHODS: &[&str] = &[
    "clone",
    "cloned",
    "copied",
    "to_vec",
    "to_owned",
    "iter",
    "into_iter",
    "as_ref",
    "as_slice",
    "as_deref",
    "collect",
    "rev",
    "first",
    "last",
    "get",
    "pop",
    "next",
];

/// Methods whose result depends on their argument (`a.eq(&over(10))`).
const ARGUMENT_COMPARING_METHODS: &[&str] = &[
    "eq",
    "ne",
    "cmp",
    "partial_cmp",
    "contains",
    "starts_with",
    "ends_with",
];

/// Assertion macros (the `claims` family) that check a variant and return
/// its payload.
const PAYLOAD_EXTRACTING_MACROS: &[&str] = &[
    "assert_ok",
    "assert_err",
    "assert_some",
    "assert_ready",
    "assert_ready_ok",
    "assert_ready_err",
];

/// #14562: methods that hand back the record they are called on (or the
/// record an `Option`/`Result` holds), so a later read of the changed field
/// still reads the owner's value.
const RECORD_KEEPING_METHODS: &[&str] = &[
    "clone",
    "to_owned",
    "as_ref",
    "as_mut",
    "as_deref",
    "borrow",
    "deref",
    "unwrap",
    "expect",
    "unwrap_or",
    "unwrap_or_default",
    "into_inner",
];

/// Free functions that hand their one argument on.
const FORWARDING_CALLEES: &[&str] = &["from", "black_box", "identity"];

/// Types whose `new` wraps its argument.
const WRAPPING_NEW_TYPES: &[&str] = &["Box", "Rc", "Arc", "Cell", "RefCell"];

/// Where the owner call's value stands in one expression.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sink {
    Absent,
    Masked,
    /// The expression's value is the owner's value or computed from it.
    Value,
    /// #14562: the owner's whole returned record, while the change sets one
    /// field of it (`Reader::kept_field`). Compared whole it reaches; a read
    /// of that field is the owner's value; any other read masks it.
    Record,
    /// The expression is a container the test built around the owner's value
    /// (tuple, array, variant payload). A shape-only read of it (`.len()`,
    /// `.is_some()`) says nothing about the payload.
    Payload,
}

impl Sink {
    fn reaches(self) -> bool {
        matches!(self, Self::Value | Self::Record | Self::Payload)
    }
}

/// The owner-value reading of `assertion_text` in `test`, for a call of the
/// function named `owner`.
///
/// #14562: `kept_field` names the one field of the record the owner returns
/// that the change sets (`big: n >= 10,`). The owner's value reaches
/// the compared operand only whole (`assert_eq!(opts(10), want)`) or through
/// that field (`opts(10).big`). A read of another field (`opts(10).retries`)
/// or a method of the record (`opts(10).summary()`) is `Masked`: it does not
/// carry the changed field. Methods that hand the record back
/// (`RECORD_KEEPING_METHODS`) keep it whole. Once the kept field is selected,
/// the rest of the operand is read as with no kept field.
pub(in crate::analysis) fn owner_field_value_flow(
    test: &TestSummary,
    assertion_text: &str,
    owner: &str,
    kept_field: Option<&str>,
) -> OwnerValueFlow {
    if owner.is_empty() {
        return OwnerValueFlow::Absent;
    }
    let Some(operands) = assertion_oracle_text(assertion_text) else {
        return OwnerValueFlow::Absent;
    };
    // The operand text is the assertion macro with its message arguments
    // dropped: `assert_eq!(a, b)`.
    let Some(ast::Expr::MacroExpr(invocation)) = parse_expression(&operands) else {
        return OwnerValueFlow::Absent;
    };
    let Some(operands) = invocation
        .macro_call()
        .and_then(|call| macro_arguments(&call))
    else {
        return OwnerValueFlow::Absent;
    };
    let reader = Reader {
        test,
        owner,
        kept_field,
    };
    let mut flow = OwnerValueFlow::Absent;
    for operand in &operands {
        match reader.text_sink(operand, 0) {
            Sink::Value | Sink::Record | Sink::Payload => return OwnerValueFlow::Reaches,
            Sink::Masked => flow = OwnerValueFlow::Masked,
            Sink::Absent => {}
        }
    }
    flow
}

/// #14701: whether `assertion_text` is a plain `assert!(owner(..).field)` or
/// `assert!(!owner(..).field)`, where `owner(..)` is the owner's whole
/// returned record read as [`owner_field_value_flow`] reads it with `field`
/// kept (directly, through a once-bound `let`, or through a method that hands
/// the record back). `assert!` takes a `bool`, so the assertion is
/// `assert_eq!(owner(..).field, true)` (or `false`) spelled another way.
pub(in crate::analysis) fn bool_field_assertion(
    test: &TestSummary,
    assertion_text: &str,
    owner: &str,
    field: &str,
) -> bool {
    if owner.is_empty() || !super::owner_pin::is_plain_macro(assertion_text, "assert") {
        return false;
    }
    let Some(operands) = assertion_oracle_text(assertion_text) else {
        return false;
    };
    let Some(ast::Expr::MacroExpr(invocation)) = parse_expression(&operands) else {
        return false;
    };
    let Some(operands) = invocation
        .macro_call()
        .and_then(|call| macro_arguments(&call))
    else {
        return false;
    };
    let [operand] = operands.as_slice() else {
        return false;
    };
    let Some(mut expr) = parse_expression(operand) else {
        return false;
    };
    let mut negations = 0;
    let expr = loop {
        expr = match expr {
            ast::Expr::ParenExpr(inner) => match inner.expr() {
                Some(inner) => inner,
                None => return false,
            },
            ast::Expr::PrefixExpr(prefix)
                if negations == 0 && prefix.op_kind() == Some(ast::UnaryOp::Not) =>
            {
                negations += 1;
                match prefix.expr() {
                    Some(inner) => inner,
                    None => return false,
                }
            }
            other => break other,
        };
    };
    let ast::Expr::FieldExpr(read) = expr else {
        return false;
    };
    if read
        .name_ref()
        .map(|name| name.text().to_string())
        .as_deref()
        != Some(field)
    {
        return false;
    }
    let reader = Reader {
        test,
        owner,
        kept_field: Some(field),
    };
    reader.inner(read.expr(), 0) == Sink::Record
}

/// The comma-separated arguments of a macro call. The split points are read
/// from text with string, char and comment contents erased, so a comma or
/// bracket inside a literal cannot split an argument; the arguments
/// themselves keep their source text.
fn macro_arguments(call: &ast::MacroCall) -> Option<Vec<String>> {
    let tokens = call.token_tree()?.syntax().text().to_string();
    let masked = mask_comments_and_strings(&tokens);
    // Masking preserves length, and the token tree keeps its one-byte
    // delimiters.
    let end = masked.len().checked_sub(1)?;
    if masked.len() != tokens.len() || end == 0 {
        return None;
    }
    // Arguments that parse as one tuple keep a turbofish whole
    // (`over::<T, U>(10)`), which a bracket count cannot see.
    if let Some(ast::Expr::TupleExpr(tuple)) =
        parse_expression(&format!("({},)", tokens.get(1..end)?))
    {
        return Some(
            tuple
                .fields()
                .map(|field| field.syntax().text().to_string().trim().to_string())
                .filter(|argument| !argument.is_empty())
                .collect(),
        );
    }
    let mut arguments = Vec::new();
    let mut depth = 0usize;
    let mut start = 1usize;
    for (offset, byte) in masked.bytes().enumerate().take(end).skip(1) {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                arguments.push(tokens.get(start..offset)?.trim().to_string());
                start = offset + 1;
            }
            _ => {}
        }
    }
    arguments.push(tokens.get(start..end)?.trim().to_string());
    arguments.retain(|argument| !argument.is_empty());
    Some(arguments)
}

struct Reader<'a> {
    test: &'a TestSummary,
    owner: &'a str,
    /// #14562: the one field of the owner's returned record the change sets.
    kept_field: Option<&'a str>,
}

impl Reader<'_> {
    /// Whether `text` holds a call of the owner outside strings and
    /// comments: the name as a whole word, then `(` or a turbofish. A bare
    /// mention (a variable or field of the same name, the function passed
    /// by name) is not a call whose value could be dropped here.
    fn mentions(&self, text: &str) -> bool {
        if !contains_as_whole_word(text, self.owner) {
            return false;
        }
        let masked = mask_comments_and_strings(text);
        let bytes = masked.as_bytes();
        let is_ident = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
        masked.match_indices(self.owner).any(|(start, name)| {
            let end = start + name.len();
            let before = start.checked_sub(1).and_then(|at| bytes.get(at));
            if before.is_some_and(|byte| is_ident(*byte))
                || bytes.get(end).is_some_and(|byte| is_ident(*byte))
            {
                return false;
            }
            let rest = masked.get(end..).unwrap_or_default().trim_start();
            rest.starts_with('(') || rest.starts_with("::<")
        })
    }

    /// The reading of an owner call itself.
    fn owner_value(&self) -> Sink {
        if self.kept_field.is_some() {
            Sink::Record
        } else {
            Sink::Value
        }
    }

    /// `Masked` when `node` names the owner anywhere, else `Absent`: the
    /// reading for a shape that carries no value this module can follow.
    fn unfollowed(&self, node: &ra_ap_syntax::SyntaxNode) -> Sink {
        if self.mentions(&node.text().to_string()) {
            Sink::Masked
        } else {
            Sink::Absent
        }
    }

    /// The strongest reading among `parts` when any one of them decides the
    /// result (operands of a comparison or arithmetic operator).
    fn any_value(&self, parts: impl IntoIterator<Item = Sink>) -> Sink {
        let mut result = Sink::Absent;
        for part in parts {
            if part.reaches() {
                return Sink::Value;
            }
            if part == Sink::Masked {
                result = Sink::Masked;
            }
        }
        result
    }

    /// A container built from `parts`.
    fn container(&self, parts: impl IntoIterator<Item = Sink>) -> Sink {
        match self.any_value(parts) {
            Sink::Value => Sink::Payload,
            other => other,
        }
    }

    fn sink(&self, expr: &ast::Expr, depth: usize) -> Sink {
        match expr {
            ast::Expr::ParenExpr(inner) => self.inner(inner.expr(), depth),
            ast::Expr::PrefixExpr(inner) => self.inner(inner.expr(), depth),
            ast::Expr::RefExpr(inner) => self.inner(inner.expr(), depth),
            ast::Expr::CastExpr(inner) => self.inner(inner.expr(), depth),
            ast::Expr::TryExpr(inner) => self.inner(inner.expr(), depth),
            ast::Expr::AwaitExpr(inner) => self.inner(inner.expr(), depth),
            ast::Expr::TupleExpr(tuple) => {
                self.container(tuple.fields().map(|field| self.sink(&field, depth)))
            }
            ast::Expr::ArrayExpr(array) => {
                self.container(array.exprs().map(|element| self.sink(&element, depth)))
            }
            ast::Expr::RecordExpr(record) => {
                let Some(fields) = record.record_expr_field_list() else {
                    return self.unfollowed(expr.syntax());
                };
                let parts = fields
                    .fields()
                    .map(|field| self.inner(field.expr(), depth))
                    .collect::<Vec<_>>();
                match (self.container(parts), fields.spread()) {
                    (Sink::Absent, Some(_)) => self.unfollowed(expr.syntax()),
                    (sink, _) => sink,
                }
            }
            ast::Expr::BinExpr(binary) => self.binary(binary, expr, depth),
            ast::Expr::CallExpr(call) => self.call(call, expr, depth),
            ast::Expr::MethodCallExpr(call) => self.method_call(call, expr, depth),
            ast::Expr::FieldExpr(field) => self.field(field, expr, depth),
            ast::Expr::IndexExpr(index) => {
                let base = self.inner(index.base(), depth);
                let position = self.inner(index.index(), depth);
                if base == Sink::Value || position.reaches() {
                    Sink::Value
                } else {
                    // An element of a test-built array is not followed.
                    self.unfollowed(expr.syntax())
                }
            }
            // The branch taken depends on the owner's value; a branch body
            // may never run, so an owner call there is not followed.
            ast::Expr::IfExpr(branch) => {
                if self.inner(branch.condition(), depth).reaches() {
                    Sink::Value
                } else {
                    self.unfollowed(expr.syntax())
                }
            }
            ast::Expr::MatchExpr(matched) => {
                let scrutinee = self.inner(matched.expr(), depth);
                // #14562: arms may destructure any field of the record.
                if scrutinee == Sink::Record {
                    Sink::Masked
                } else if scrutinee.reaches() {
                    Sink::Value
                } else {
                    self.unfollowed(expr.syntax())
                }
            }
            // A block's value is its tail; a statement's value is discarded.
            ast::Expr::BlockExpr(block) => {
                let tail = block
                    .stmt_list()
                    .and_then(|statements| statements.tail_expr());
                match self.inner(tail, depth) {
                    sink if sink.reaches() => sink,
                    _ => self.unfollowed(expr.syntax()),
                }
            }
            ast::Expr::MacroExpr(invocation) => self.macro_call(invocation, expr, depth),
            ast::Expr::PathExpr(path) => self.binding(path, depth),
            _ => self.unfollowed(expr.syntax()),
        }
    }

    /// The reading of `text` as one expression; text that does not parse is
    /// not followed.
    fn text_sink(&self, text: &str, depth: usize) -> Sink {
        match parse_expression(text) {
            Some(parsed) => self.sink(&parsed, depth),
            None if self.mentions(text) => Sink::Masked,
            None => Sink::Absent,
        }
    }

    fn inner(&self, expr: Option<ast::Expr>, depth: usize) -> Sink {
        expr.map_or(Sink::Absent, |expr| self.sink(&expr, depth))
    }

    fn binary(&self, binary: &ast::BinExpr, expr: &ast::Expr, depth: usize) -> Sink {
        use ast::BinaryOp;
        match binary.op_kind() {
            // `a || b` holds whenever one side does, so neither side's value
            // is established as deciding it.
            Some(BinaryOp::LogicOp(ast::LogicOp::Or))
            | Some(BinaryOp::Assignment { .. })
            | None => self.unfollowed(expr.syntax()),
            Some(_) => self.any_value([
                self.inner(binary.lhs(), depth),
                self.inner(binary.rhs(), depth),
            ]),
        }
    }

    fn call(&self, call: &ast::CallExpr, expr: &ast::Expr, depth: usize) -> Sink {
        let callee = match call.expr() {
            Some(ast::Expr::PathExpr(path)) => path.path(),
            _ => None,
        };
        let Some(callee) = callee else {
            return self.unfollowed(expr.syntax());
        };
        let Some(name) = callee
            .segment()
            .and_then(|segment| segment.name_ref())
            .map(|name| name.text().to_string())
        else {
            return self.unfollowed(expr.syntax());
        };
        if name == self.owner {
            return self.owner_value();
        }
        let arguments = call
            .arg_list()
            .map(|list| list.args().collect::<Vec<_>>())
            .unwrap_or_default();
        let parts = arguments
            .iter()
            .map(|argument| self.sink(argument, depth))
            .collect::<Vec<_>>();
        // `Some(x)`, `Ok(x)`, `Wrapper(x)`: a variant or tuple-struct payload.
        if name.starts_with(|first: char| first.is_ascii_uppercase()) {
            return self.container(parts);
        }
        let qualifier = callee
            .qualifier()
            .and_then(|qualifier| qualifier.segment())
            .and_then(|segment| segment.name_ref())
            .map(|name| name.text().to_string());
        if name == "new"
            && qualifier
                .as_deref()
                .is_some_and(|qualifier| WRAPPING_NEW_TYPES.contains(&qualifier))
        {
            return self.container(parts);
        }
        // A function may ignore an argument when it has another input to
        // answer from (`first(true, over(10))`). A one-argument call is read
        // as forwarding; so are the conversion spellings.
        if arguments.len() == 1 || FORWARDING_CALLEES.contains(&name.as_str()) {
            return self.any_value(parts);
        }
        self.unfollowed(expr.syntax())
    }

    fn method_call(&self, call: &ast::MethodCallExpr, expr: &ast::Expr, depth: usize) -> Sink {
        let Some(name) = call.name_ref().map(|name| name.text().to_string()) else {
            return self.unfollowed(expr.syntax());
        };
        if name == self.owner {
            return self.owner_value();
        }
        let receiver = self.inner(call.receiver(), depth);
        // `masked.eq(&over(10))`: a comparing method's result depends on its
        // argument whatever the receiver is.
        if ARGUMENT_COMPARING_METHODS.contains(&name.as_str())
            && receiver != Sink::Absent
            && call.arg_list().is_some_and(|list| {
                list.args()
                    .any(|argument| self.sink(&argument, depth).reaches())
            })
        {
            return Sink::Value;
        }
        match receiver {
            // #14562: a method of the whole record may never read the
            // changed field; only the methods that hand the record back keep
            // it.
            Sink::Record if RECORD_KEEPING_METHODS.contains(&name.as_str()) => Sink::Record,
            // `over(10).eq(&want)` compares the whole record.
            Sink::Record if matches!(name.as_str(), "eq" | "ne" | "cmp" | "partial_cmp") => {
                Sink::Value
            }
            Sink::Record => Sink::Masked,
            // Any method of the owner's own value reads that value.
            Sink::Value => Sink::Value,
            Sink::Payload if UNWRAPPING_METHODS.contains(&name.as_str()) => Sink::Value,
            // `Some(over(10)).eq(&Some(true))`, `vec![over(10)].contains(&true)`:
            // the comparison reads the payload.
            Sink::Payload if ARGUMENT_COMPARING_METHODS.contains(&name.as_str()) => Sink::Value,
            Sink::Payload if PAYLOAD_KEEPING_METHODS.contains(&name.as_str()) => Sink::Payload,
            Sink::Payload | Sink::Masked => Sink::Masked,
            Sink::Absent => {
                let parts = call
                    .arg_list()
                    .map(|list| {
                        list.args()
                            .map(|argument| self.sink(&argument, depth))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if ARGUMENT_COMPARING_METHODS.contains(&name.as_str()) {
                    self.any_value(parts)
                } else {
                    // `Some(true).unwrap_or(over(10))`, a lazy closure
                    // argument: the receiver decides whether it is used.
                    self.unfollowed(expr.syntax())
                }
            }
        }
    }

    fn field(&self, field: &ast::FieldExpr, expr: &ast::Expr, depth: usize) -> Sink {
        let mut base = field.expr();
        while let Some(ast::Expr::ParenExpr(inner)) = &base {
            base = inner.expr();
        }
        // `(over(10), 1).0` selects the element; `.1` drops the call.
        if let Some(ast::Expr::TupleExpr(tuple)) = &base {
            let selected = field
                .name_ref()
                .and_then(|name| name.text().to_string().parse::<usize>().ok())
                .and_then(|position| tuple.fields().nth(position));
            return match selected.map(|element| self.sink(&element, depth)) {
                Some(sink) if sink.reaches() => sink,
                _ => self.unfollowed(expr.syntax()),
            };
        }
        let base = self.inner(base, depth);
        // #14562: of the whole record, only the changed field carries it.
        if base == Sink::Record {
            let name = field.name_ref().map(|name| name.text().to_string());
            return if name.as_deref() == self.kept_field {
                Sink::Value
            } else {
                Sink::Masked
            };
        }
        match base {
            Sink::Value => Sink::Value,
            Sink::Absent => Sink::Absent,
            Sink::Record | Sink::Payload | Sink::Masked => Sink::Masked,
        }
    }

    fn macro_call(&self, invocation: &ast::MacroExpr, expr: &ast::Expr, depth: usize) -> Sink {
        let unfollowed = self.unfollowed(expr.syntax());
        if unfollowed == Sink::Absent {
            return Sink::Absent;
        }
        let Some(call) = invocation.macro_call() else {
            return unfollowed;
        };
        let name = call
            .path()
            .and_then(|path| path.segment())
            .and_then(|segment| segment.name_ref())
            .map(|name| name.text().to_string());
        let Some(items) = macro_arguments(&call) else {
            return unfollowed;
        };
        let read = |item: &String| self.text_sink(item, depth);
        let sink = match name.as_deref() {
            // `matches!(scrutinee, pattern)`.
            Some("matches") => match items.first().map(read) {
                // #14562: a pattern over the record observes the changed
                // field only when it names it.
                Some(Sink::Record) => {
                    let names_kept = self.kept_field.is_some_and(|kept| {
                        items
                            .iter()
                            .skip(1)
                            .any(|pattern| contains_as_whole_word(pattern, kept))
                    });
                    if names_kept {
                        Sink::Value
                    } else {
                        Sink::Masked
                    }
                }
                Some(sink) if sink.reaches() => Sink::Value,
                _ => Sink::Masked,
            },
            Some("vec") => self.container(items.iter().map(read)),
            // `claims`-style extraction (`let v = assert_ok!(parse(5));`)
            // asserts the variant and returns its payload, so the result is
            // the owner's value again.
            Some(name) if PAYLOAD_EXTRACTING_MACROS.contains(&name) => {
                match items.first().map(read) {
                    // #14562: the extracted payload is still the record.
                    Some(Sink::Record) => Sink::Record,
                    Some(sink) if sink.reaches() => Sink::Value,
                    _ => Sink::Masked,
                }
            }
            // Every argument is formatted into the result.
            Some("format") => self.any_value(items.iter().map(read)),
            Some("dbg") => match items.as_slice() {
                [item] => read(item),
                _ => Sink::Masked,
            },
            _ => Sink::Masked,
        };
        if sink == Sink::Absent {
            unfollowed
        } else {
            sink
        }
    }

    /// A bare name the test body binds exactly once with a plain `let`.
    fn binding(&self, path: &ast::PathExpr, depth: usize) -> Sink {
        let Some(name) = path
            .path()
            .filter(|path| path.qualifier().is_none())
            .and_then(|path| path.segment())
            .filter(|segment| segment.generic_arg_list().is_none())
            .and_then(|segment| segment.name_ref())
            .map(|name| name.text().to_string())
        else {
            return Sink::Absent;
        };
        if depth >= LET_DEPTH
            || self
                .test
                .let_bindings
                .iter()
                .filter(|binding| binding.name == name)
                .count()
                != 1
        {
            return Sink::Absent;
        }
        let Some(parse) = parse_body(&self.test.body) else {
            return Sink::Absent;
        };
        let mut initializers = parse
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::LetStmt::cast)
            .filter(|statement| {
                matches!(
                    statement.pat(),
                    Some(ast::Pat::IdentPat(pattern))
                        if pattern.pat().is_none()
                            && pattern.name().is_some_and(|bound| bound.text() == name.as_str())
                )
            });
        let (Some(statement), None) = (initializers.next(), initializers.next()) else {
            return Sink::Absent;
        };
        if statement.let_else().is_some() {
            return self.unfollowed(statement.syntax());
        }
        self.inner(statement.initializer(), depth + 1)
    }
}

/// `text` parsed as one expression.
fn parse_expression(text: &str) -> Option<ast::Expr> {
    let wrapped = format!("fn ripr_operand() {{ let _ = {text}\n; }}");
    let parse = parse_clean_source_file_memoized(&wrapped)?;
    parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::LetStmt::cast)?
        .initializer()
}

/// The test body as a source file: the whole `fn` item, or its statements
/// wrapped in one.
fn parse_body(body: &str) -> Option<ra_ap_syntax::Parse<ra_ap_syntax::SourceFile>> {
    parse_clean_source_file_memoized(body)
        .or_else(|| parse_clean_source_file_memoized(&format!("fn ripr_body() {{\n{body}\n}}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::syntax::ra::shadow_facts_for_body_text;

    fn flow(body: &str, assertion: &str) -> OwnerValueFlow {
        kept_flow(body, assertion, None)
    }

    fn kept_flow(body: &str, assertion: &str, kept_field: Option<&str>) -> OwnerValueFlow {
        let (_, let_bindings) = shadow_facts_for_body_text(body);
        let test = TestSummary {
            name: "t".to_string(),
            body: body.to_string().into(),
            assertions: Vec::new(),
            file: "src/lib.rs".into(),
            start_line: 1,
            end_line: 1,
            calls: crate::analysis::facts::SharedFactSlice::empty(),
            literals: crate::analysis::facts::SharedFactSlice::empty(),
            attrs: crate::analysis::facts::SharedFactSlice::empty(),
            nested_fn_names: crate::analysis::facts::SharedFactSlice::empty(),
            let_bindings: crate::analysis::facts::SharedFactSlice::from_vec(let_bindings),
            generated: None,
        };
        owner_field_value_flow(&test, assertion, "over", kept_field)
    }

    /// #14701: only a plain `assert!` whose whole condition is the kept field
    /// read off the owner's record (optionally negated once) is a field pin.
    #[test]
    fn a_bare_assert_on_the_kept_field_is_a_field_pin() {
        let pins = |body: &str, assertion: &str| {
            let (_, let_bindings) = shadow_facts_for_body_text(body);
            let test = TestSummary {
                name: "t".to_string(),
                body: body.to_string().into(),
                assertions: Vec::new(),
                file: "src/lib.rs".into(),
                start_line: 1,
                end_line: 1,
                calls: crate::analysis::facts::SharedFactSlice::empty(),
                literals: crate::analysis::facts::SharedFactSlice::empty(),
                attrs: crate::analysis::facts::SharedFactSlice::empty(),
                nested_fn_names: crate::analysis::facts::SharedFactSlice::empty(),
                let_bindings: crate::analysis::facts::SharedFactSlice::from_vec(let_bindings),
                generated: None,
            };
            bool_field_assertion(&test, assertion, "over", "big")
        };
        for assertion in [
            "assert!(over(10).big);",
            "assert!(!over(9).big);",
            "assert!(over(10).big, \"big at ten\");",
            "assert!((over(10).big));",
            "assert!(!(over(9).big));",
            "assert!(over(10).clone().big);",
            "assert!(over(10).unwrap().big);",
        ] {
            assert!(pins(assertion, assertion), "{assertion}");
        }
        let body = "let o = over(10);\nassert!(o.big);";
        assert!(pins(body, "assert!(o.big);"));
        for assertion in [
            "assert!(over(10).retries);",
            "assert!(over(10).big && ok);",
            "assert!(over(10).big || true);",
            "assert!(!!over(10).big);",
            "assert!(over(10).big == true);",
            "assert!(over(10).inner.big);",
            "assert!(wrap(over(10)).big);",
            "assert!(other(10).big);",
            "assert!(over(10).big());",
            "debug_assert!(over(10).big);",
            "assert_eq!(over(10).big, true);",
            "assert!(matches!(over(10), Opts { big: true, .. }));",
            "my::assert!(over(10).big);",
        ] {
            assert!(!pins(assertion, assertion), "{assertion}");
        }
    }

    /// #14562: with the changed field `big` kept, the owner's record reaches
    /// the compared operand whole or through `big` only.
    #[test]
    fn a_kept_field_is_read_only_whole_or_through_that_field() {
        for assertion in [
            "assert_eq!(over(10).big, true);",
            "assert!(over(10).big);",
            "assert!(!over(10).big);",
            "assert_eq!(over(10), Opts { retries: 3, big: true });",
            "assert_eq!(over(10).clone().big, true);",
            "assert_eq!(over(10).unwrap().big, true);",
            "assert_eq!(over(10).big.to_string(), \"true\");",
            "assert!(over(10).eq(&want));",
            "assert!(matches!(over(10), Opts { big: true, .. }));",
            "assert_eq!(over(10).borrow().big, true);",
            "assert_eq!(wrap(over(10)).inner.big, true);",
        ] {
            assert_eq!(
                kept_flow(assertion, assertion, Some("big")),
                OwnerValueFlow::Reaches,
                "{assertion}"
            );
        }
        for assertion in [
            "assert_eq!(over(10).retries, 3);",
            "assert_eq!(over(10).unwrap().retries, 3);",
            "assert_eq!(over(10).summary(), \"x\");",
            "assert_eq!(over(10).retries.big, 3);",
            "assert_eq!(Some(over(10).retries).unwrap(), 3);",
            "assert_eq!(assert_ok!(over(10)).retries, 3);",
            "assert_eq!(over(10).0, 3);",
            "assert_eq!(match over(10) { Opts { retries, .. } => retries }, 3);",
            "assert!(matches!(over(10), Opts { retries: 3, .. }));",
        ] {
            assert_eq!(
                kept_flow(assertion, assertion, Some("big")),
                OwnerValueFlow::Masked,
                "{assertion}"
            );
            // Without a kept field the same assertion reads the owner's value.
            assert_eq!(
                kept_flow(assertion, assertion, None),
                OwnerValueFlow::Reaches,
                "{assertion}"
            );
        }
        let body = "let o = over(10);\nassert_eq!(o.retries, 3);";
        assert_eq!(
            kept_flow(body, "assert_eq!(o.retries, 3);", Some("big")),
            OwnerValueFlow::Masked
        );
        let body = "let o = over(10);\nassert_eq!(o.big, true);";
        assert_eq!(
            kept_flow(body, "assert_eq!(o.big, true);", Some("big")),
            OwnerValueFlow::Reaches
        );
    }

    fn one(assertion: &str) -> OwnerValueFlow {
        flow(assertion, assertion)
    }

    #[test]
    fn value_reaching_shapes_read_reaches() {
        for assertion in [
            "assert_eq!(over(10), true);",
            "assert!(over(10));",
            "assert!(!over(9));",
            "assert!(over(10) == true);",
            "assert_eq!(over(10) as u8, 1);",
            "assert_eq!((over(10), over(9)), (true, false));",
            "assert_eq!(Some(over(10)), Some(true));",
            "assert_eq!(!over(10), false);",
            "assert_eq!(over(10).then_some(1), Some(1));",
            "assert_eq!(if over(10) { 1 } else { 2 }, 1);",
            "assert_eq!(match over(10) { true => 'y', false => 'n' }, 'y');",
            "assert_eq!((over(10), 1).0, true);",
            "assert_eq!(Some(over(10)).unwrap(), true);",
            "assert_eq!(vec![over(10), over(9)], vec![true, false]);",
            "assert_eq!(format!(\"{}, {}\", 1, over(10)), \"1, true\");",
            "assert!(matches!(over(10), true));",
            "assert_eq!(u8::from(over(10)), 1);",
            "assert_eq!(cart.over(10), true);",
            "assert_eq!(Cart::over(&cart, 10), true);",
            "assert!(ready && over(10));",
            "assert!(true.eq(&over(10)));",
            "assert!(Some(over(10)).eq(&Some(true)));",
            "assert!(vec![over(10)].contains(&true));",
            "assert_eq!(Some(over(10)).unwrap_or(false), true);",
            "assert_eq!([over(10)].first(), Some(&true));",
            "assert_eq!(vec![over(10)].pop(), Some(true));",
            "assert_eq!(format!(\"a, b ({})\", over(10)), \"a, b (true)\");",
            "assert_eq!(match over(10) { true => ',', false => 'n' }, ',');",
            "assert_eq!(over(10), true, \"got {}\", Some(over(10)).is_some());",
            // #14276 review: a turbofish comma does not split the operand,
            // and a comparing method reads its argument past a masked receiver.
            "assert_eq!(over::<u8, u16>(10), true);",
            "assert!(Some(over(9)).is_some().eq(&over(10)));",
        ] {
            assert_eq!(one(assertion), OwnerValueFlow::Reaches, "{assertion}");
        }
    }

    #[test]
    fn discarded_or_masked_owner_values_read_masked() {
        for assertion in [
            "assert_eq!(Some(over(10)).is_some(), true);",
            "assert_eq!(vec![over(10)].len(), 1);",
            "assert_eq!((over(10), 1).1, 1);",
            "assert_eq!({ over(10); true }, true);",
            "assert_eq!(first(true, over(10)), true);",
            "assert_eq!(Some(true).unwrap_or(over(10)), true);",
            "assert_eq!(Some(true).unwrap_or_else(|| over(10)), true);",
            "assert_eq!(r.unwrap_or_else(|_| over(10)), true);",
            "assert_eq!(false.then(|| over(10)), None);",
            "assert_eq!(o.map(|_| over(10)), None);",
            "assert_eq!(None::<u32>.map_or(true, |_| over(10)), true);",
            "assert_eq!(if true { true } else { over(10) }, true);",
            "assert_eq!(std::iter::repeat_with(|| over(10)).take(0).count(), 0);",
            "assert!(true || over(10));",
            "assert_eq!([over(10)].len(), 1);",
            "assert_eq!(Some(over(10)).iter().count(), 1);",
            "assert_eq!([over(10)].first().is_some(), true);",
            "assert_eq!(Some(over(10)).map(|_| 1), Some(1));",
        ] {
            assert_eq!(one(assertion), OwnerValueFlow::Masked, "{assertion}");
        }
    }

    #[test]
    fn an_assertion_without_an_owner_call_reads_absent() {
        for assertion in [
            "assert_eq!(label(10), \"big\");",
            "assert_eq!(true, true);",
            "assert_eq!(1, 1, \"over(10) was {}\", over(10));",
            "assert_eq!(recover(10), true);",
            "assert_that!(over(10)).is_true();",
            "assert_eq!(first(true, over), true);",
            "assert_eq!(first(config.over, 1), true);",
            "assert_eq!(first(\"over(10)\", 1), true);",
        ] {
            assert_eq!(one(assertion), OwnerValueFlow::Absent, "{assertion}");
        }
    }

    #[test]
    fn a_single_let_binding_is_followed_to_its_initializer() {
        assert_eq!(
            flow(
                "let a = over(10); assert_eq!(a, true);",
                "assert_eq!(a, true);"
            ),
            OwnerValueFlow::Reaches
        );
        assert_eq!(
            flow(
                "let v = match Some(1) { Some(_) => true, None => over(10) }; assert_eq!(v, true);",
                "assert_eq!(v, true);"
            ),
            OwnerValueFlow::Masked
        );
        assert_eq!(
            flow(
                "let a = over(10); let b = Some(a); assert_eq!(b.is_some(), true);",
                "assert_eq!(b.is_some(), true);"
            ),
            OwnerValueFlow::Masked
        );
        // A `claims` extraction returns the payload it asserted on.
        assert_eq!(
            flow(
                "let v = assert_ok!(over(10)); assert_eq!(v, true);",
                "assert_eq!(v, true);"
            ),
            OwnerValueFlow::Reaches
        );
        assert_eq!(
            flow(
                "let v = assert_some!(Some(over(10))); assert_eq!(v, true);",
                "assert_eq!(v, true);"
            ),
            OwnerValueFlow::Reaches
        );
        // Control: a macro outside the list keeps the masked reading.
        assert_eq!(
            flow(
                "let v = assert_matches!(over(10), true); assert_eq!(v, ());",
                "assert_eq!(v, ());"
            ),
            OwnerValueFlow::Masked
        );
    }

    #[test]
    fn a_name_bound_more_than_once_or_by_a_pattern_is_not_followed() {
        assert_eq!(
            flow(
                "let v = Some(over(10)).is_some();\nlet v = true;\nassert_eq!(v, true);",
                "assert_eq!(v, true);"
            ),
            OwnerValueFlow::Absent
        );
        assert_eq!(
            flow(
                "let (a, _b) = (over(10), 1); assert_eq!(a, true);",
                "assert_eq!(a, true);"
            ),
            OwnerValueFlow::Absent
        );
    }
}
