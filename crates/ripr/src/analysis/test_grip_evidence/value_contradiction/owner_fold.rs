//! One owner version read for the both-versions fold (#15272).
//!
//! The integer fold reads a free function of scalar result. Two more owner
//! shapes let a test pass identically on a changed owner and its base side:
//!
//! - a method reading its receiver's fields (`self.base * 3`), called on a
//!   record literal the test spells (`Acc { base: 0 }.double()`);
//! - a tuple tail (`(a, a >= 0)`), compared whole (`pair(5) == (5, true)`).
//!
//! Both are read by rewriting the owner text into free functions the scalar
//! fold already evaluates: `self.field` becomes a parameter, and a tuple tail
//! becomes one function per component. Nothing here evaluates anything new.
//!
//! Both readings support "the versions agree at this call" and "the versions
//! differ somewhere". Neither supports "this exact assertion separates them":
//! the callers leave a differing receiver call or tuple unread, so the
//! readings can withhold credit and cannot grant it.

use super::{Value, fold_expression, fold_function_value, strip_parens};
use crate::analysis::syntax::parse_clean_source_file;
use crate::analysis::syntax::parse_clean_source_file_memoized;
use ra_ap_syntax::{
    AstNode, SyntaxKind, TextRange,
    ast::{self, HasArgList, HasGenericArgs},
};
use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet};

/// What an owner returns at one call.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Folded {
    Scalar(Value),
    /// Each component of a tuple tail, in order.
    Tuple(Vec<Value>),
}

/// The parameter a receiver field becomes in the free form. The fold reads
/// names from an environment, so the prefix only has to miss real parameters.
fn field_parameter(field: &str) -> String {
    format!("ripr_receiver_field_{field}")
}

/// One owner version in free-function form.
pub(super) struct OwnerFold {
    /// The owner as a free function: its own text, or a method's text with
    /// the receiver replaced by one parameter per field it reads.
    text: String,
    /// The receiver fields the free form takes ahead of the owner's own
    /// parameters, sorted; `None` for an owner without a receiver.
    receiver_fields: Option<Vec<String>>,
    /// The owner's own parameter count, the receiver aside.
    parameters: usize,
    /// One free function per component of a tuple tail, read on first use.
    components: OnceCell<Option<Vec<String>>>,
}

impl OwnerFold {
    /// `None` when the text is not one cleanly parsed function, or when a
    /// receiver is used as anything but the base of a plain field read.
    pub(super) fn read(body: &str) -> Option<Self> {
        // One owner is read once per changed line it holds; the memo keeps
        // its current text from being parsed again each time.
        let parse = parse_clean_source_file_memoized(body)?;
        let function = parse
            .tree()
            .syntax()
            .descendants()
            .find_map(ast::Fn::cast)?;
        let param_list = function.param_list()?;
        let parameters = param_list.params().count();
        let Some(receiver) = param_list.self_param() else {
            return Some(Self {
                text: body.to_string(),
                receiver_fields: None,
                parameters,
                components: OnceCell::new(),
            });
        };
        // `&mut self` and `mut self` may write a field before reading it, and
        // a typed receiver (`self: Box<Self>`) reads through another type.
        if receiver.mut_token().is_some() || receiver.ty().is_some() {
            return None;
        }
        let mut fields = BTreeSet::new();
        let mut edits: Vec<(TextRange, String)> = Vec::new();
        let receiver_range = receiver.syntax().text_range();
        for token in function
            .syntax()
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| token.kind() == SyntaxKind::SELF_KW)
        {
            if receiver_range.contains_range(token.text_range()) {
                continue;
            }
            // `self` must be the whole base of `self.name`: a bare `self`
            // (passed on, compared, returned), `self.0`, `self.method()` or a
            // `self` inside a macro's tokens is not a field value.
            let path_expr = token.parent_ancestors().find_map(ast::PathExpr::cast)?;
            if path_expr.syntax().text_range() != token.text_range() {
                return None;
            }
            let field = path_expr.syntax().parent().and_then(ast::FieldExpr::cast)?;
            let name = field.name_ref()?;
            if name.syntax().text().to_string().parse::<u64>().is_ok() {
                return None;
            }
            let name = name.syntax().text().to_string();
            edits.push((field.syntax().text_range(), field_parameter(&name)));
            fields.insert(name);
        }
        let fields: Vec<String> = fields.into_iter().collect();
        // The receiver becomes the field parameters. Their type is left to
        // inference: the fold then reads them at no declared width.
        let field_parameters = fields
            .iter()
            .map(|field| format!("{}: _", field_parameter(field)))
            .collect::<Vec<_>>()
            .join(", ");
        let receiver_edit = match (fields.is_empty(), param_list.params().next()) {
            // No field read: drop the receiver and its comma.
            (true, Some(first)) => (
                TextRange::new(receiver_range.start(), first.syntax().text_range().start()),
                String::new(),
            ),
            _ => (receiver_range, field_parameters),
        };
        edits.push(receiver_edit);
        Some(Self {
            text: apply_edits(body, edits)?,
            receiver_fields: Some(fields),
            parameters,
            components: OnceCell::new(),
        })
    }

    /// The owner's value at `arguments`, called on a receiver whose fields
    /// are `receiver` when it has one. A receiver given to an owner without
    /// one, a missing one, or a field the receiver does not spell is `None`.
    pub(super) fn at(
        &self,
        receiver: Option<&BTreeMap<String, i128>>,
        arguments: &[i128],
    ) -> Option<Folded> {
        let mut values = Vec::with_capacity(arguments.len());
        match (&self.receiver_fields, receiver) {
            (None, None) => {}
            (Some(fields), Some(receiver)) => {
                for field in fields {
                    values.push(*receiver.get(field)?);
                }
            }
            _ => return None,
        }
        values.extend_from_slice(arguments);
        if let Some(value) = fold_function_value(&self.text, &values) {
            return Some(Folded::Scalar(value));
        }
        self.components
            .get_or_init(|| tuple_tail_components(&self.text))
            .as_ref()?
            .iter()
            .map(|component| fold_function_value(component, &values))
            .collect::<Option<Vec<_>>>()
            .map(Folded::Tuple)
    }

    /// The receiver fields this version reads; empty without a receiver.
    pub(super) fn receiver_fields(&self) -> &[String] {
        self.receiver_fields.as_deref().unwrap_or_default()
    }

    pub(super) fn has_receiver(&self) -> bool {
        self.receiver_fields.is_some()
    }

    pub(super) fn parameters(&self) -> usize {
        self.parameters
    }
}

/// `text` with each range replaced; `None` when two ranges overlap.
fn apply_edits(text: &str, mut edits: Vec<(TextRange, String)>) -> Option<String> {
    edits.sort_by_key(|(range, _)| range.start());
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    for (range, replacement) in edits {
        let (start, end) = (usize::from(range.start()), usize::from(range.end()));
        if start < cursor {
            return None;
        }
        out.push_str(text.get(cursor..start)?);
        out.push_str(&replacement);
        cursor = end;
    }
    out.push_str(text.get(cursor..)?);
    Some(out)
}

/// One function text per component when the function's tail is a tuple of
/// two or more components and its declared return type is a tuple of the
/// same arity: the tail and the return type are each replaced by the
/// component's. Statements ahead of the tail stay, so a `return` among them
/// fails the component's fold rather than being skipped.
fn tuple_tail_components(text: &str) -> Option<Vec<String>> {
    let parse = parse_clean_source_file(text)?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    let ast::Expr::TupleExpr(tail) = function.body()?.tail_expr()? else {
        return None;
    };
    let ast::Type::TupleType(declared) = function.ret_type()?.ty()? else {
        return None;
    };
    let values: Vec<ast::Expr> = tail.fields().collect();
    let types: Vec<ast::Type> = declared.fields().collect();
    if values.len() < 2 || values.len() != types.len() {
        return None;
    }
    values
        .iter()
        .zip(&types)
        .map(|(value, ty)| {
            apply_edits(
                text,
                vec![
                    (
                        declared.syntax().text_range(),
                        ty.syntax().text().to_string(),
                    ),
                    (
                        tail.syntax().text_range(),
                        value.syntax().text().to_string(),
                    ),
                ],
            )
        })
        .collect()
}

/// The receiver fields and the argument texts when `operand` is exactly
/// `Type { field: literal, .. }.name(args)`: a record literal of the owner's
/// self type, every spelled field an integer the fold reads. A parenthesized
/// literal reads the same. `None` for any
/// other receiver: a binding, a constructor call or a nested expression may
/// hold field values the assertion does not show.
pub(super) fn literal_receiver_call(
    operand: &str,
    self_type: &str,
    name: &str,
) -> Option<(BTreeMap<String, i128>, Vec<String>)> {
    let wrapped = format!("fn ripr_operand() {{ let _ = {operand}\n; }}");
    let parse = parse_clean_source_file_memoized(&wrapped)?;
    let initializer = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::LetStmt::cast)?
        .initializer()?;
    let ast::Expr::MethodCallExpr(call) = initializer else {
        return None;
    };
    if call.name_ref()?.syntax().text() != name || call.generic_arg_list().is_some() {
        return None;
    }
    let ast::Expr::RecordExpr(record) = strip_parens(call.receiver()?)? else {
        return None;
    };
    let path = record.path()?;
    if path.qualifier().is_some() || path.segment()?.syntax().text() != self_type {
        return None;
    }
    // A functional update (`..rest`) needs no refusal of its own: a field it
    // supplies is not spelled, and a field the owner reads but the literal
    // does not spell leaves the call unread.
    let list = record.record_expr_field_list()?;
    let environment = BTreeMap::new();
    let mut fields = BTreeMap::new();
    for field in list.fields() {
        let name = field.name_ref()?.syntax().text().to_string();
        // Shorthand (`Acc { base }`) reads a binding, not a literal.
        let value = fold_expression(&field.expr()?.syntax().text().to_string(), &environment)?;
        if fields.insert(name, value).is_some() {
            return None;
        }
    }
    let arguments = call
        .arg_list()?
        .args()
        .map(|argument| argument.syntax().text().to_string())
        .collect();
    Some((fields, arguments))
}

#[cfg(test)]
mod tests {
    use super::{Folded, OwnerFold, Value, literal_receiver_call, tuple_tail_components};
    use std::collections::BTreeMap;

    fn receiver(fields: &[(&str, i128)]) -> BTreeMap<String, i128> {
        fields
            .iter()
            .map(|(name, value)| ((*name).to_string(), *value))
            .collect()
    }

    #[test]
    fn a_method_folds_through_the_receiver_fields_it_reads() -> Result<(), String> {
        let fold =
            OwnerFold::read("fn scale(&self, by: u64) -> u64 { self.base * by + self.bias }")
                .ok_or("method reads")?;
        assert!(fold.has_receiver());
        assert_eq!(fold.receiver_fields(), ["base", "bias"]);
        assert_eq!(fold.parameters(), 1);
        assert_eq!(
            fold.at(Some(&receiver(&[("base", 4), ("bias", 1)])), &[3]),
            Some(Folded::Scalar(Value::Int(13)))
        );
        // A field the receiver does not spell, and a call with no receiver.
        assert_eq!(fold.at(Some(&receiver(&[("base", 4)])), &[3]), None);
        assert_eq!(fold.at(None, &[4, 1, 3]), None);
        Ok(())
    }

    #[test]
    fn a_method_that_reads_no_field_drops_the_receiver() -> Result<(), String> {
        for text in [
            "fn twice(&self, x: u32) -> u32 { x * 2 }",
            "fn twice(&self) -> u32 { 2 }",
        ] {
            let fold = OwnerFold::read(text).ok_or("method reads")?;
            let arguments: &[i128] = if fold.parameters() == 1 { &[5] } else { &[] };
            assert!(
                fold.at(Some(&BTreeMap::new()), arguments).is_some(),
                "{text}"
            );
            assert_eq!(fold.at(None, arguments), None, "{text}");
        }
        Ok(())
    }

    #[test]
    fn a_free_function_takes_no_receiver() -> Result<(), String> {
        let fold = OwnerFold::read("fn twice(x: u32) -> u32 { x * 2 }").ok_or("function reads")?;
        assert!(!fold.has_receiver());
        assert_eq!(fold.at(None, &[5]), Some(Folded::Scalar(Value::Int(10))));
        assert_eq!(fold.at(Some(&BTreeMap::new()), &[5]), None);
        Ok(())
    }

    #[test]
    fn a_receiver_used_as_more_than_a_field_base_is_not_read() {
        for text in [
            // May write the field before the read.
            "fn bump(&mut self) -> u64 { self.base * 3 }",
            "fn bump(mut self) -> u64 { self.base * 3 }",
            // Reads through another type.
            "fn boxed(self: Box<Self>) -> u64 { self.base * 3 }",
            // The receiver itself, a tuple field, a method and macro tokens.
            "fn whole(&self) -> u64 { helper(self) }",
            "fn first(&self) -> u64 { self.0 * 3 }",
            "fn via(&self) -> u64 { self.base() * 3 }",
            "fn logged(&self) -> u64 { trace!(self.base); self.base * 3 }",
        ] {
            assert!(OwnerFold::read(text).is_none(), "{text}");
        }
    }

    #[test]
    fn a_tuple_tail_folds_one_component_at_a_time() -> Result<(), String> {
        let text = "fn pair(a: i32) -> (i32, bool) { (a, a >= 0) }";
        let fold = OwnerFold::read(text).ok_or("function reads")?;
        assert_eq!(
            fold.at(None, &[0]),
            Some(Folded::Tuple(vec![Value::Int(0), Value::Bool(true)]))
        );
        assert_eq!(
            tuple_tail_components(text),
            Some(vec![
                "fn pair(a: i32) -> i32 { a }".to_string(),
                "fn pair(a: i32) -> bool { a >= 0 }".to_string(),
            ])
        );
        // The component keeps its declared width: 200 + 100 is no `u8`.
        let narrow = OwnerFold::read("fn wide(a: u8) -> (u8, u8) { (a, a + 100) }")
            .ok_or("function reads")?;
        assert_eq!(narrow.at(None, &[200]), None);
        Ok(())
    }

    #[test]
    fn a_tail_that_is_not_a_declared_tuple_has_no_components() {
        for text in [
            // The declared type hides the arity.
            "fn pair(a: i32) -> Pair { (a, a >= 0) }",
            "fn pair(a: i32) -> (i32, bool, u8) { (a, a >= 0) }",
            // One component is a parenthesized value, not a tuple.
            "fn one(a: i32) -> (i32) { (a) }",
            "fn unit(a: i32) { a; }",
        ] {
            assert_eq!(tuple_tail_components(text), None, "{text}");
        }
    }

    #[test]
    fn only_a_record_literal_of_the_self_type_is_a_read_receiver() {
        assert_eq!(
            literal_receiver_call("Acc { base: 0, step: 1 + 1 }.double(3)", "Acc", "double"),
            Some((receiver(&[("base", 0), ("step", 2)]), vec!["3".to_string()]))
        );
        assert_eq!(
            literal_receiver_call("(Acc { base: 0 }).double()", "Acc", "double"),
            Some((receiver(&[("base", 0)]), Vec::new()))
        );
        // A functional update supplies fields the literal does not spell.
        assert_eq!(
            literal_receiver_call("Acc { base: 0, ..rest }.double()", "Acc", "double"),
            Some((receiver(&[("base", 0)]), Vec::new()))
        );
        for operand in [
            // Another type, a qualified path, another method.
            "Twin { base: 0 }.double()",
            "other::Acc { base: 0 }.double()",
            "Acc { base: 0 }.triple()",
            // A receiver whose fields the operand does not show.
            "acc.double()",
            "Acc::new(0).double()",
            "Acc { base }.double()",
            "Acc { base: zero() }.double()",
            "Acc { base: 0, base: 1 }.double()",
            // More than the call.
            "Acc { base: 0 }.double() + 1",
            "Acc { base: 0 }.double().min(1)",
            "wrap(Acc { base: 0 }.double())",
            "Acc { base: 0 }.double::<u8>()",
        ] {
            assert_eq!(
                literal_receiver_call(operand, "Acc", "double"),
                None,
                "{operand}"
            );
        }
    }
}
