//! Whole returned match observation (#5500), distinct from arm selection.
//!
//! The diff and parser must identify a newly added, unconditional root owner
//! whose entire synchronous body is this match. Inputs may be opaque: the
//! receipt observes only the returned value of the exact asserted invocation.

use super::*;
use crate::analysis::diagnostic_origin::ParserByteSpan;
use crate::analysis::diff::{ChangedFile, reconstruct_old_source};
use ra_ap_syntax::ast::HasGenericParams;

pub(in crate::analysis) struct WholeTailMatchObservation {
    pin: OwnerReturnPin,
}

impl WholeTailMatchObservation {
    pub(in crate::analysis) fn establish(
        probe: &Probe,
        owner: &FunctionSummary,
        index: &RustIndex,
        changed: &ChangedFile,
        span: ParserByteSpan,
        workspace_complete: bool,
    ) -> Option<Self> {
        if !workspace_complete
            || probe.family != ProbeFamily::MatchArm
            || probe.before.is_some()
            || probe.owner.as_ref() != Some(&owner.id)
            || !probe.location.file.ends_with(&owner.file)
            || changed.path != owner.file
            || index
                .functions()
                .iter()
                .filter(|f| f.id == owner.id)
                .count()
                != 1
        {
            return None;
        }
        let facts = index.files().get(&owner.file)?;
        if facts.used_lexical_fallback {
            return None;
        }
        let source = facts.source.as_ref();
        let parse = parse_clean_source_file(source)?;
        let tree = parse.tree();
        // A file-level condition can remove the owner too.
        if tree.attrs().next().is_some() {
            return None;
        }
        let functions = tree
            .syntax()
            .children()
            .filter_map(ast::Fn::cast)
            .filter(|f| f.name().is_some_and(|n| n.text() == owner.name))
            .collect::<Vec<_>>();
        let [function] = functions.as_slice() else {
            return None;
        };
        if function.async_token().is_some()
            || function.const_token().is_some()
            || function.unsafe_token().is_some()
            || function.abi().is_some()
            || function.generic_param_list().is_some()
            || function.attrs().next().is_some()
            || !owner.item.has_body
            || !returns_primitive_scalar(owner, index)
        {
            return None;
        }
        let start = usize::from(function.fn_token()?.text_range().start());
        let end = usize::from(function.syntax().text_range().end());
        let line = |offset: usize| source[..offset].bytes().filter(|b| *b == b'\n').count() + 1;
        if line(start) != owner.start_line || source.get(start..end)? != owner.body {
            return None;
        }
        let body = function.body()?.stmt_list()?;
        if body.statements().next().is_some() {
            return None;
        }
        let ast::Expr::MatchExpr(matched) = body.tail_expr()? else {
            return None;
        };
        let token = matched.match_token()?;
        let token_start = usize::from(token.text_range().start());
        let scrutinee = matched.expr()?;
        let header_end = usize::from(scrutinee.syntax().text_range().end());
        // Exact contiguous parser text and byte origin, not a line-name search.
        if token_start != span.start_byte
            || line(token_start) != probe.location.line
            || source.get(token_start..header_end)? != probe.expression
            || probe.after.as_deref() != Some(probe.expression.as_str())
        {
            return None;
        }
        // Every owner line is an exact addition, not a changed/renamed owner
        // borrowing a current tail that was already present on the old side.
        let end_line = line(end.saturating_sub(1));
        for owner_line in owner.start_line..=end_line {
            let mut additions = changed
                .added_lines
                .iter()
                .filter(|added| added.line == owner_line);
            let only = additions.next()?;
            if additions.next().is_some() || only.new_side_line != owner_line {
                return None;
            }
        }
        if changed
            .removed_lines
            .iter()
            .any(|removed| (owner.start_line..=end_line).contains(&removed.new_side_line))
        {
            return None;
        }
        let old_source = reconstruct_old_source(source, changed)?;
        let old = parse_clean_source_file(&old_source)?;
        if old
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::Fn::cast)
            .any(|f| f.name().is_some_and(|n| n.text() == owner.name))
        {
            return None;
        }
        Some(Self {
            pin: OwnerReturnPin::from_owner_path(owner, index, ReturnPathGate::Any)?,
        })
    }

    pub(in crate::analysis) fn admits(
        &self,
        test: &TestSummary,
        assertion: &OracleFact,
        index: &RustIndex,
        imports_foreign: ForeignImport<'_>,
        syntax: &OwnerPinSyntax,
    ) -> bool {
        // Keep this new adapter independent of opaque expected bindings,
        // including macro/static writes tracked by a separate owner-pin claim.
        let Some(operands) = assertion_comparison_operands(&assertion.text) else {
            return false;
        };
        if !operands
            .iter()
            .any(|operand| independent_expected(operand, test, index, &self.pin.owner_file))
        {
            return false;
        }
        self.pin
            .admits(test, assertion, index, imports_foreign, syntax)
    }
}

fn independent_expected(
    operand: &str,
    test: &TestSummary,
    index: &RustIndex,
    owner_file: &Path,
) -> bool {
    let operand = operand.trim();
    // Parsing the entire operand prevents a literal prefix from admitting an
    // expression or a macro that calculates its expected result via the owner.
    let Some(parse) = parse_clean_source_file(&format!("fn expected() {{ {operand} }}")) else {
        return false;
    };
    let Some(function) = parse.tree().syntax().children().find_map(ast::Fn::cast) else {
        return false;
    };
    let Some(expression) = function.body().and_then(|body| body.tail_expr()) else {
        return false;
    };
    if expression.syntax().text().to_string() != operand {
        return false;
    }
    if matches!(expression, ast::Expr::Literal(_))
        || matches!(&expression, ast::Expr::PrefixExpr(prefix)
        if prefix.op_kind() == Some(ast::UnaryOp::Neg) && matches!(prefix.expr(), Some(ast::Expr::Literal(_))))
    {
        return true;
    }
    let Some(primitive) = operand
        .strip_suffix("::MAX")
        .filter(|primitive| INTEGER_TYPES.contains(primitive))
    else {
        return false;
    };
    // `u64` is rebindable even though it normally names a primitive. Inspect
    // the assertion's own file, not just the owner's return-type file.
    index.files().get(&test.file).is_some_and(|facts| {
        let Some(parsed) = parse_clean_source_file(&facts.source) else {
            return false;
        };
        !parsed
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::Name::cast)
            .any(|name| name.text().trim_start_matches("r#") == primitive)
            && !flattened_use_paths(&facts.source).iter().any(|import| {
                ((import.path.ends_with("::*") || import.path == "*")
                    && !(test.file == owner_file
                        && matches!(import.path.as_str(), "super::*" | "self::*")))
                    || import
                        .alias
                        .as_deref()
                        .unwrap_or_else(|| import.path.rsplit("::").next().unwrap_or_default())
                        == primitive
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::diff::ChangedLine;
    use crate::analysis::probes::SeededProbe;
    use crate::analysis::rust_index::summarize_file;

    fn fixture() -> Result<(RustIndex, ChangedFile, SeededProbe), String> {
        let path = PathBuf::from("src/lib.rs");
        let source = "struct Lease { elapsed: Option<u64> }\n/// Whole returned match.\npub(crate) fn charge(lease: &Lease) -> u64 {\n    match lease.elapsed {\n        Some(ms) => ms,\n        None => 600,\n    }\n}\n";
        let facts = summarize_file(path.clone(), source.to_string());
        let mut index = RustIndex::default();
        index.extend_functions(facts.functions.iter().cloned());
        index.insert_file_only(path.clone(), facts);
        let changed = ChangedFile {
            path,
            added_lines: source
                .lines()
                .enumerate()
                .skip(1)
                .map(|(i, text)| ChangedLine {
                    line: i + 1,
                    new_side_line: i + 1,
                    text: text.to_string(),
                })
                .collect(),
            removed_lines: Vec::new(),
        };
        let seeded = crate::analysis::probes::probes_for_file_with_relations(
            Path::new("."),
            &changed,
            &index,
            &[],
        )
        .into_iter()
        .find(|s| s.probe.expression == "match lease.elapsed")
        .ok_or("fixture must seed the exact header")?;
        Ok((index, changed, seeded))
    }

    #[test]
    fn receipt_refuses_incomplete_wrong_owner_ast_source_and_diff() -> Result<(), String> {
        let (index, changed, seeded) = fixture()?;
        let owner = index
            .functions()
            .iter()
            .find(|f| f.name == "charge")
            .ok_or("indexed owner")?;
        let span = seeded.parser_span.ok_or("exact parser custody")?;
        let establish =
            |probe: &Probe, owner: &FunctionSummary, changed: &ChangedFile, span, complete| {
                WholeTailMatchObservation::establish(probe, owner, &index, changed, span, complete)
                    .is_some()
            };
        assert!(establish(&seeded.probe, owner, &changed, span, true));
        assert!(!establish(&seeded.probe, owner, &changed, span, false));
        assert!(!establish(
            &seeded.probe,
            owner,
            &changed,
            ParserByteSpan {
                start_byte: span.start_byte + 1
            },
            true
        ));
        for case in ["name", "body", "start", "id", "file"] {
            let mut other = owner.clone();
            match case {
                "name" => other.name = "other".to_string(),
                "body" => other.body = other.body.replace("600", "601"),
                "start" => other.start_line += 1,
                "id" => other.id.0.push_str("-wrong"),
                "file" => other.file = PathBuf::from("src/other.rs"),
                _ => return Err("unknown owner case".to_string()),
            }
            assert!(
                !establish(&seeded.probe, &other, &changed, span, true),
                "{case}"
            );
        }
        for case in ["owner", "line", "expression", "after", "before", "file"] {
            let mut other = seeded.probe.clone();
            match case {
                "owner" => other.owner = None,
                "line" => other.location.line += 1,
                "expression" => other.expression = "match other.elapsed".to_string(),
                "after" => other.after = Some("match other.elapsed".to_string()),
                "before" => other.before = Some(other.expression.clone()),
                "file" => other.location.file = PathBuf::from("src/other.rs"),
                _ => return Err("unknown probe case".to_string()),
            }
            assert!(!establish(&other, owner, &changed, span, true), "{case}");
        }
        for case in ["path", "text", "missing", "duplicate", "old"] {
            let mut other = changed.clone();
            match case {
                "path" => other.path = PathBuf::from("src/other.rs"),
                "text" => other.added_lines[0].text = "/// mismatched source".to_string(),
                "missing" => {
                    other.added_lines.remove(1);
                }
                "duplicate" => other.added_lines.push(other.added_lines[1].clone()),
                "old" => other.removed_lines.push(ChangedLine {
                    line: 2,
                    new_side_line: 3,
                    text: "fn charge(_: &Lease) -> u64 { 0 }".to_string(),
                }),
                _ => return Err("unknown diff case".to_string()),
            }
            assert!(
                !establish(&seeded.probe, owner, &other, span, true),
                "{case}"
            );
        }
        Ok(())
    }
}
