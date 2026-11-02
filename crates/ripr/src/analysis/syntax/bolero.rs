//! Closed syntax of the audited Bolero runner-to-property profile (#10758).
//!
//! This is not provider authority. The facts producer may register a receipt
//! only after a fresh resolved registry edge. Unmatched chains stay limited;
//! local `bolero`/`check`/`for_each` lookalikes acquire no property authority.

use super::parse_clean_source_file;
use ra_ap_syntax::ast::{self, HasAttrs, HasGenericArgs, HasName};
use ra_ap_syntax::{AstNode, SyntaxNode};

const MAX_CHAIN_BYTES: usize = 8 * 1024;

/// One `check!().with_type::<T>().for_each(..)` candidate in a test body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BoleroProfile {
    pub(crate) function_line: usize,
    pub(crate) function_name: String,
    pub(crate) for_each_line: usize,
    /// The single type argument of the qualified `with_type::<T>()` call.
    pub(crate) input_type: String,
    /// The chain reached a terminal `for_each` under a qualified check root.
    pub(crate) complete: bool,
}

/// Whether `closure` is the single argument of a qualified Bolero
/// `for_each` and the chain's terminal invocation sits on an eager path
/// candidate in `function`.
pub(crate) fn bolero_for_each_consumer(
    closure: &ast::ClosureExpr,
    function: &ast::Fn,
) -> Option<SyntaxNode> {
    let for_each = bolero_for_each_call(closure)?;
    let body = function.body()?;
    if !body
        .syntax()
        .text_range()
        .contains_range(for_each.syntax().text_range())
    {
        return None;
    }
    let file = function
        .syntax()
        .ancestors()
        .find(|node| ast::SourceFile::can_cast(node.kind()))?;
    let source = file.text().to_string();
    let bolero_bound = file_binds_bolero_check(&source);
    let (input_type, complete) = qualified_chain(for_each.clone(), bolero_bound)?;
    if !complete || input_type.is_empty() {
        return None;
    }
    let parent = for_each.syntax().parent()?;
    Some(parent)
}

pub(crate) fn bolero_profiles(source: &str) -> Vec<BoleroProfile> {
    let Some(parsed) = parse_clean_source_file(source) else {
        return Vec::new();
    };
    let bolero_bound = file_binds_bolero_check(source);
    let lines = super::ra::LineIndex::new(source);
    let mut profiles = Vec::new();
    for function in parsed
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter(has_test_attribute)
    {
        let Some(name) = function.name().map(|name| name.text().to_string()) else {
            continue;
        };
        let Some(body) = function.body() else {
            continue;
        };
        for node in body.syntax().descendants() {
            let Some(for_each) = ast::MethodCallExpr::cast(node.clone()) else {
                continue;
            };
            if for_each
                .name_ref()
                .is_none_or(|name| name.text() != "for_each")
            {
                continue;
            }
            let Some((input_type, complete)) = qualified_chain(for_each.clone(), bolero_bound)
            else {
                continue;
            };
            profiles.push(BoleroProfile {
                function_line: lines.line(function.syntax().text_range().start()),
                function_name: name.clone(),
                for_each_line: lines.line(for_each.syntax().text_range().start()),
                input_type,
                complete,
            });
        }
    }
    profiles
}

fn bolero_for_each_call(closure: &ast::ClosureExpr) -> Option<ast::MethodCallExpr> {
    if closure.async_token().is_some()
        || closure.const_token().is_some()
        || closure.gen_token().is_some()
        || closure
            .syntax()
            .children()
            .any(|node| ast::Attr::can_cast(node.kind()))
    {
        return None;
    }
    let params = closure.param_list()?.params().count();
    if params != 1 {
        return None;
    }
    let expression = unparenthesized(closure);
    let args = expression.parent().and_then(ast::ArgList::cast)?;
    if args.args().count() != 1 {
        return None;
    }
    if args
        .args()
        .next()
        .map(|first| first.syntax().clone())
        .as_ref()
        != Some(&expression)
    {
        return None;
    }
    let for_each = args.syntax().parent().and_then(ast::MethodCallExpr::cast)?;
    for_each
        .name_ref()
        .is_some_and(|name| name.text() == "for_each")
        .then_some(for_each)
}

fn has_test_attribute(function: &ast::Fn) -> bool {
    function.attrs().any(|attr| {
        attr.syntax()
            .text()
            .to_string()
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .any(|ch| ch == '#')
            && attr
                .syntax()
                .text()
                .to_string()
                .replace([' ', '\n', '\t'], "")
                .starts_with("#[test")
    })
}

fn qualified_chain(for_each: ast::MethodCallExpr, bolero_bound: bool) -> Option<(String, bool)> {
    if usize::from(for_each.syntax().text_range().len()) > MAX_CHAIN_BYTES {
        return None;
    }
    let mut receiver = for_each.receiver()?;
    let mut input_type = None;
    loop {
        match receiver {
            ast::Expr::MethodCallExpr(call)
                if call
                    .name_ref()
                    .is_some_and(|name| name.text() == "with_type") =>
            {
                if input_type.is_some() {
                    return None;
                }
                input_type = Some(single_type_argument(&call)?);
                receiver = call.receiver()?;
            }
            ast::Expr::MethodCallExpr(_) => return None,
            ast::Expr::MacroExpr(expression) => {
                let macro_call = expression.macro_call()?;
                if !is_bolero_check(&macro_call, bolero_bound) {
                    return None;
                }
                let ty = input_type?;
                return Some((ty, true));
            }
            _ => return None,
        }
    }
}

fn single_type_argument(call: &ast::MethodCallExpr) -> Option<String> {
    let generics = call.generic_arg_list()?;
    let mut args = generics.generic_args();
    let first = args.next()?;
    if args.next().is_some() {
        return None;
    }
    match first {
        ast::GenericArg::TypeArg(type_arg) => Some(compact(&type_arg.syntax().text().to_string())),
        _ => None,
    }
}

fn is_bolero_check(call: &ast::MacroCall, bolero_bound: bool) -> bool {
    let Some(path) = call
        .path()
        .map(|path| compact(&path.syntax().text().to_string()))
    else {
        return false;
    };
    matches!(
        path.as_str(),
        "bolero::check" | "::bolero::check" | "bolero::check!" | "::bolero::check!"
    ) || (bolero_bound && matches!(path.as_str(), "check" | "check!"))
}

fn file_binds_bolero_check(source: &str) -> bool {
    if source.contains("mod bolero") || source.contains("macro_rules! check") {
        return false;
    }
    source.lines().any(|line| {
        let line = line.split("//").next().unwrap_or(line).trim();
        line.starts_with("use bolero")
            || line.starts_with("use ::bolero")
            || line.contains("use bolero::")
    })
}

fn compact(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn unparenthesized(closure: &ast::ClosureExpr) -> SyntaxNode {
    let mut expression = closure.syntax().clone();
    while let Some(parent) = expression
        .parent()
        .filter(|node| ast::ParenExpr::can_cast(node.kind()))
    {
        expression = parent;
    }
    expression
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
fn increment(n: u8) -> u16 {
    u16::from(n) + 1
}

#[test]
fn increment_matches_model() {
    bolero::check!()
        .with_type::<u8>()
        .for_each(|n| {
            assert_eq!(increment(*n), u16::from(*n) + 1);
        });
}
"#;

    #[test]
    fn qualified_chain_is_admitted() {
        let profiles = bolero_profiles(GOOD);
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].input_type, "u8");
        assert!(profiles[0].complete);
    }

    #[test]
    fn builder_without_for_each_is_not_complete() {
        let source = GOOD.replace(
            ".for_each(|n| {\n            assert_eq!(increment(*n), u16::from(*n) + 1);\n        });",
            ";",
        );
        let profiles = bolero_profiles(&source);
        assert!(
            profiles.is_empty() || profiles.iter().all(|profile| !profile.complete),
            "builder-only must not complete the profile"
        );
    }

    #[test]
    fn local_check_lookalike_is_ignored() {
        let source = r#"
macro_rules! check { ($($t:tt)*) => {}; }
#[test]
fn fake() {
    check!().with_type::<u8>().for_each(|n| { assert_eq!(*n, 1); });
}
"#;
        assert!(bolero_profiles(source).is_empty());
    }

    #[test]
    fn unsupported_engine_method_refuses_the_chain() {
        let source = GOOD.replace(
            ".with_type::<u8>()",
            ".with_type::<u8>().with_generator(/* unsupported */)",
        );
        assert!(bolero_profiles(&source).is_empty());
    }

    #[test]
    fn eager_path_accepts_bolero_for_each_closure() -> Result<(), &'static str> {
        let parsed = parse_clean_source_file(GOOD).ok_or("parse")?;
        let function = parsed
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::Fn::cast)
            .find(|function| {
                function
                    .name()
                    .is_some_and(|name| name.text() == "increment_matches_model")
            })
            .ok_or("test function")?;
        let closure = function
            .body()
            .ok_or("body")?
            .syntax()
            .descendants()
            .find_map(ast::ClosureExpr::cast)
            .ok_or("closure")?;
        assert!(bolero_for_each_consumer(&closure, &function).is_some());
        Ok(())
    }
}
