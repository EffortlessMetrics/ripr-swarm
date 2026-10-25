//! Bounded arithmetic discrimination for the returned-closure pin, not a
//! general evaluator. Unknown bindings, types and operations withhold credit.

use super::{split_arguments, split_trailing_group};
use crate::analysis::rust_index::{FunctionSummary, RustIndex};
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasAttrs, HasGenericParams, HasName},
};

#[derive(Debug)]
pub(in crate::analysis::classify) struct ArithmeticPin {
    owner_arity: usize,
    parameters: Vec<String>,
    kind: NumberKind,
    expression: Expression,
    operators: Vec<char>,
}

impl ArithmeticPin {
    pub(in crate::analysis::classify) fn establish(
        owner: &FunctionSummary,
        index: &RustIndex,
    ) -> Option<Self> {
        let source = &index.files().get(&owner.file)?.source;
        let parse = parse_clean_source_file(source)?;
        let mut candidates = parse
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::Fn::cast)
            .filter(|function| {
                let range = function.syntax().text_range();
                let start = source[..usize::from(range.start())].matches('\n').count() + 1;
                let end = source[..usize::from(range.end())].matches('\n').count() + 1;
                function
                    .name()
                    .is_some_and(|name| name.text() == owner.name.as_str())
                    && start <= owner.start_line
                    && owner.start_line <= end
            });
        let function = candidates.next()?;
        if candidates.next().is_some() || function.generic_param_list().is_some() {
            return None;
        }
        let signature: String = function
            .ret_type()?
            .ty()?
            .syntax()
            .text()
            .to_string()
            .split_whitespace()
            .collect();
        let (inputs, output) = signature.strip_prefix("impl")?.split_once("->")?;
        let (selected_trait, arguments) = inputs.split_once('(')?;
        if !["Fn", "FnMut", "FnOnce"].contains(&selected_trait) {
            return None;
        }
        let arguments = arguments.strip_suffix(')')?;
        let kind = NumberKind::parse(output)?;
        let closure_types = split_arguments(arguments)?;
        if closure_types.iter().any(|ty| *ty != kind.name) {
            return None;
        }
        let params = function.param_list()?;
        if params.self_param().is_some() {
            return None;
        }
        let mut parameters = Vec::new();
        for parameter in params.params() {
            if parameter.ty()?.syntax().text().to_string().trim() != kind.name {
                return None;
            }
            let ast::Pat::IdentPat(ident) = parameter.pat()? else {
                return None;
            };
            parameters.push(ident.name()?.text().trim_start_matches("r#").to_string());
        }
        let owner_arity = parameters.len();
        let ast::Expr::ClosureExpr(closure) = function.body()?.tail_expr()? else {
            return None;
        };
        for parameter in closure.param_list()?.params() {
            let ast::Pat::IdentPat(ident) = parameter.pat()? else {
                return None;
            };
            parameters.push(ident.name()?.text().trim_start_matches("r#").to_string());
        }
        if parameters.len() != owner_arity + closure_types.len() {
            return None;
        }

        // Only lexical item scopes can supply primitive names. Test-local
        // imports and sibling module contents cannot rebind the owner's type.
        for scope in function.syntax().ancestors().skip(1) {
            if let Some(implementation) = ast::Impl::cast(scope.clone()) {
                if implementation.generic_param_list().is_some() {
                    return None;
                }
                continue;
            }
            // A module does not inherit its parent's bare item bindings.
            if ast::Module::can_cast(scope.kind()) {
                break;
            }
            if !(ast::SourceFile::can_cast(scope.kind())
                || ast::ItemList::can_cast(scope.kind())
                || ast::AssocItemList::can_cast(scope.kind()))
            {
                return None;
            }
            let protected = [kind.name, selected_trait];
            for item in scope.children().filter_map(ast::Item::cast) {
                let introduced = match &item {
                    ast::Item::Enum(item) => item.name(),
                    ast::Item::Module(item) => item.name(),
                    ast::Item::Struct(item) => item.name(),
                    ast::Item::Trait(item) => item.name(),
                    ast::Item::TypeAlias(item) => item.name(),
                    ast::Item::Union(item) => item.name(),
                    _ => None,
                };
                if introduced
                    .is_some_and(|name| protected.contains(&name.text().trim_start_matches("r#")))
                {
                    return None;
                }
                match item {
                    ast::Item::MacroCall(_) => return None,
                    ast::Item::Use(import)
                        if import_binds_protected(&import.use_tree()?, None, &protected, 0)? =>
                    {
                        return None;
                    }
                    ast::Item::ExternCrate(external) => {
                        let name = if let Some(rename) = external.rename() {
                            if rename.underscore_token().is_some() {
                                continue;
                            }
                            rename.name()?.text().trim_start_matches("r#").to_string()
                        } else {
                            external
                                .name_ref()?
                                .text()
                                .trim_start_matches("r#")
                                .to_string()
                        };
                        if protected.contains(&name.as_str()) {
                            return None;
                        }
                    }

                    _ => {}
                }
            }
        }

        let holding_scope = function.syntax().parent()?;
        let mut operators = Vec::new();
        let expression = Expression::parse(
            &closure.body()?,
            &parameters,
            kind,
            &holding_scope,
            &mut operators,
            0,
        )?;
        if operators.is_empty() || operators.len() > 8 {
            return None;
        }
        Some(Self {
            owner_arity,
            parameters,
            kind,
            expression,
            operators,
        })
    }

    pub(in crate::analysis::classify) fn discriminates(&self, operand: &str) -> bool {
        let Some(values) = self.inputs(operand) else {
            return false;
        };
        // Distinct written decimals can round to one `f32`/`f64`, so the
        // pairwise-different requirement is judged on the typed values.
        let repeated = values
            .iter()
            .enumerate()
            .any(|(at, value)| values[..at].iter().any(|seen| seen.same_value(*value)));
        if repeated {
            return false;
        }
        let Some(original) = self.expression.evaluate(&values, self.kind, None) else {
            return false;
        };
        for (node, original_op) in self.operators.iter().enumerate() {
            for replacement in ['+', '-', '*', '/', '%'] {
                if replacement == *original_op {
                    continue;
                }
                // Overflow, division by zero or a non-finite result is
                // unestablished here; do not assume a runtime panic profile.
                let Some(changed) =
                    self.expression
                        .evaluate(&values, self.kind, Some((node, replacement)))
                else {
                    return false;
                };
                if original.same_value(changed) {
                    return false;
                }
            }
        }
        true
    }

    fn inputs(&self, operand: &str) -> Option<Vec<Number>> {
        let (prefix, applied) = split_trailing_group(operand.trim())?;
        let (_, owner) = split_trailing_group(prefix)?;
        let owner = split_arguments(owner)?;
        let applied = split_arguments(applied)?;
        if owner.len() != self.owner_arity || owner.len() + applied.len() != self.parameters.len() {
            return None;
        }
        owner
            .into_iter()
            .chain(applied)
            .map(|text| self.kind.literal(text))
            .collect()
    }
}

// Import bindings are syntax identities: comments, raw identifiers and
// aliases cannot change the terminal name. `self` inherits its list prefix.
fn import_binds_protected(
    tree: &ast::UseTree,
    inherited: Option<&str>,
    protected: &[&str],
    depth: usize,
) -> Option<bool> {
    if depth > 16 {
        return None;
    }
    if tree.star_token().is_some() {
        return Some(true);
    }
    if let Some(rename) = tree.rename() {
        if rename.underscore_token().is_some() {
            return Some(false);
        }
        return Some(protected.contains(&rename.name()?.text().trim_start_matches("r#")));
    }
    let terminal = if let Some(path) = tree.path() {
        let segment = path.segment()?;
        let text = segment.syntax().text().to_string();
        if text == "self" {
            Some(inherited.unwrap_or("self").to_string())
        } else if matches!(text.as_str(), "crate" | "super") {
            Some(text)
        } else {
            Some(
                segment
                    .name_ref()?
                    .text()
                    .trim_start_matches("r#")
                    .to_string(),
            )
        }
    } else {
        inherited.map(str::to_string)
    };
    if let Some(list) = tree.use_tree_list() {
        for child in list.use_trees() {
            if import_binds_protected(&child, terminal.as_deref(), protected, depth + 1)? {
                return Some(true);
            }
        }
        return Some(false);
    }
    Some(protected.contains(&terminal?.as_str()))
}

#[derive(Clone, Copy, Debug)]
struct NumberKind {
    name: &'static str,
    signed_bits: Option<u32>,
    unsigned_bits: Option<u32>,
}

impl NumberKind {
    fn parse(text: &str) -> Option<Self> {
        let name = [
            "i8", "i16", "i32", "i64", "i128", "u8", "u16", "u32", "u64", "u128", "f32", "f64",
        ]
        .into_iter()
        .find(|name| *name == text)?;
        Some(Self {
            name,
            signed_bits: name.strip_prefix('i').and_then(|n| n.parse().ok()),
            unsigned_bits: name.strip_prefix('u').and_then(|n| n.parse().ok()),
        })
    }
    fn literal(self, text: &str) -> Option<Number> {
        let text = text.trim();
        let (negative, body) = text
            .strip_prefix('-')
            .map_or((false, text), |rest| (true, rest.trim_start()));
        if !body.starts_with(|ch: char| ch.is_ascii_digit()) {
            return None;
        }
        let end = body
            .find(|ch: char| !(ch.is_ascii_digit() || ch == '_' || ch == '.'))
            .unwrap_or(body.len());
        let (number, suffix) = body.split_at(end);
        if !suffix.is_empty() && suffix != self.name {
            return None;
        }
        let mut number: String = number.chars().filter(|ch| *ch != '_').collect();
        if negative {
            number.insert(0, '-');
        }
        let value = if self.signed_bits.is_some() {
            Number::Signed(number.parse().ok()?)
        } else if self.unsigned_bits.is_some() {
            Number::Unsigned(number.parse().ok()?)
        } else if self.name == "f32" {
            Number::Float32(number.parse::<f32>().ok()?.to_bits())
        } else {
            Number::Float64(number.parse::<f64>().ok()?.to_bits())
        };
        self.checked(value)
    }
    fn checked(self, value: Number) -> Option<Number> {
        let valid = match value {
            Number::Signed(value) => self.signed_bits.is_some_and(|bits| {
                bits == 128 || (-(1i128 << (bits - 1))..(1i128 << (bits - 1))).contains(&value)
            }),
            Number::Unsigned(value) => self
                .unsigned_bits
                .is_some_and(|bits| bits == 128 || value < (1u128 << bits)),
            Number::Float32(bits) => self.name == "f32" && f32::from_bits(bits).is_finite(),
            Number::Float64(bits) => self.name == "f64" && f64::from_bits(bits).is_finite(),
        };
        valid.then_some(value)
    }
}

#[derive(Clone, Copy, Debug)]
enum Number {
    Signed(i128),
    Unsigned(u128),
    Float32(u32),
    Float64(u64),
}

impl Number {
    fn same_value(self, other: Self) -> bool {
        match (self, other) {
            (Self::Signed(a), Self::Signed(b)) => a == b,
            (Self::Unsigned(a), Self::Unsigned(b)) => a == b,
            // Exact Rust equality, including signed zero. A tolerance could
            // conceal a distinct returned value; non-finite values refuse upstream.
            (Self::Float32(a), Self::Float32(b)) => {
                f32::from_bits(a).partial_cmp(&f32::from_bits(b)) == Some(std::cmp::Ordering::Equal)
            }
            (Self::Float64(a), Self::Float64(b)) => {
                f64::from_bits(a).partial_cmp(&f64::from_bits(b)) == Some(std::cmp::Ordering::Equal)
            }
            _ => false,
        }
    }
    fn binary(self, op: char, right: Self, kind: NumberKind) -> Option<Self> {
        macro_rules! checked {
            ($a:expr, $b:expr, $variant:ident) => {
                Self::$variant(match op {
                    '+' => $a.checked_add($b),
                    '-' => $a.checked_sub($b),
                    '*' => $a.checked_mul($b),
                    '/' => $a.checked_div($b),
                    '%' => $a.checked_rem($b),
                    _ => None,
                }?)
            };
        }
        macro_rules! float {
            ($a:expr, $b:expr, $variant:ident) => {
                Self::$variant(
                    match op {
                        '+' => $a + $b,
                        '-' => $a - $b,
                        '*' => $a * $b,
                        '/' => $a / $b,
                        '%' => $a % $b,
                        _ => return None,
                    }
                    .to_bits(),
                )
            };
        }
        kind.checked(match (self, right) {
            (Self::Signed(a), Self::Signed(b)) => checked!(a, b, Signed),
            (Self::Unsigned(a), Self::Unsigned(b)) => checked!(a, b, Unsigned),
            (Self::Float32(a), Self::Float32(b)) => {
                float!(f32::from_bits(a), f32::from_bits(b), Float32)
            }
            (Self::Float64(a), Self::Float64(b)) => {
                float!(f64::from_bits(a), f64::from_bits(b), Float64)
            }
            _ => return None,
        })
    }
    fn negate(self, kind: NumberKind) -> Option<Self> {
        kind.checked(match self {
            Self::Signed(value) => Self::Signed(value.checked_neg()?),
            Self::Float32(bits) => Self::Float32((-f32::from_bits(bits)).to_bits()),
            Self::Float64(bits) => Self::Float64((-f64::from_bits(bits)).to_bits()),
            Self::Unsigned(_) => return None,
        })
    }
}

#[derive(Debug)]
enum Expression {
    Literal(Number),
    Parameter(usize),
    Negated(Box<Self>),
    Binary {
        node: usize,
        op: char,
        left: Box<Self>,
        right: Box<Self>,
    },
}

impl Expression {
    fn parse(
        expression: &ast::Expr,
        parameters: &[String],
        kind: NumberKind,
        scope: &ra_ap_syntax::SyntaxNode,
        operators: &mut Vec<char>,
        depth: usize,
    ) -> Option<Self> {
        if depth > 16 || operators.len() > 8 {
            return None;
        }
        match expression {
            ast::Expr::Literal(literal) => Some(Self::Literal(
                kind.literal(&literal.syntax().text().to_string())?,
            )),
            ast::Expr::ParenExpr(paren) => Self::parse(
                &paren.expr()?,
                parameters,
                kind,
                scope,
                operators,
                depth + 1,
            ),
            ast::Expr::PrefixExpr(prefix) if prefix.op_token()?.text() == "-" => {
                Some(Self::Negated(Box::new(Self::parse(
                    &prefix.expr()?,
                    parameters,
                    kind,
                    scope,
                    operators,
                    depth + 1,
                )?)))
            }
            ast::Expr::PathExpr(path) => {
                let path = path.path()?;
                if path.qualifier().is_some() {
                    return None;
                }
                let name = path
                    .segment()?
                    .name_ref()?
                    .text()
                    .trim_start_matches("r#")
                    .to_string();
                // The closure's later binding shadows the owner's binding.
                if let Some(position) = parameters.iter().rposition(|known| *known == name) {
                    return Some(Self::Parameter(position));
                }
                let mut declarations =
                    scope
                        .children()
                        .filter_map(ast::Const::cast)
                        .filter(|item| {
                            item.name()
                                .is_some_and(|ident| ident.text().trim_start_matches("r#") == name)
                        });
                let constant = declarations.next()?;
                if declarations.next().is_some()
                    || constant.attrs().next().is_some()
                    || constant.ty()?.syntax().text().to_string().trim() != kind.name
                {
                    return None;
                }
                Some(Self::Literal(
                    kind.literal(
                        &constant
                            .syntax()
                            .children()
                            .find_map(ast::Expr::cast)?
                            .syntax()
                            .text()
                            .to_string(),
                    )?,
                ))
            }
            ast::Expr::BinExpr(binary) => {
                let token = binary.op_token()?;
                let text = token.text();
                let op = text.chars().next()?;
                if text.len() != 1 || !['+', '-', '*', '/', '%'].contains(&op) {
                    return None;
                }
                let node = operators.len();
                operators.push(op);
                Some(Self::Binary {
                    node,
                    op,
                    left: Box::new(Self::parse(
                        &binary.lhs()?,
                        parameters,
                        kind,
                        scope,
                        operators,
                        depth + 1,
                    )?),
                    right: Box::new(Self::parse(
                        &binary.rhs()?,
                        parameters,
                        kind,
                        scope,
                        operators,
                        depth + 1,
                    )?),
                })
            }
            // In particular, an uppercase callee is never a constant read.
            _ => None,
        }
    }
    fn evaluate(
        &self,
        values: &[Number],
        kind: NumberKind,
        replacement: Option<(usize, char)>,
    ) -> Option<Number> {
        match self {
            Self::Literal(value) => Some(*value),
            Self::Parameter(position) => values.get(*position).copied(),
            Self::Negated(inner) => inner.evaluate(values, kind, replacement)?.negate(kind),
            Self::Binary {
                node,
                op,
                left,
                right,
            } => {
                let op = replacement
                    .filter(|(changed, _)| changed == node)
                    .map_or(*op, |(_, replacement)| replacement);
                left.evaluate(values, kind, replacement)?.binary(
                    op,
                    right.evaluate(values, kind, replacement)?,
                    kind,
                )
            }
        }
    }
}
