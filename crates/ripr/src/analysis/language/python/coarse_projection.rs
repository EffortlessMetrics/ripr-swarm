//! Coarse-projection detection for owner calls inside an assertion
//! (RIPR-SPEC-0238 rule 30).
//!
//! `assertEqual(f(2) is None, False)`, `assertEqual(f(2) > 0, True)`,
//! `assert price(3) % 3 == 0`, `assertEqual(f(2).__class__.__name__, 'int')`
//! and `assertEqual(len(str(f(2))), 1)` compare with `==` and so look like
//! exact-value oracles, but what they compare is a coarse property of the
//! owner's result (its nullness, sign, residue, class name or length), which
//! the changed value usually shares with the old one. A projection that the
//! edit leaves unchanged cannot notice the edit, so the assertion earns no
//! `exposed` credit; the owner stays `weakly_exposed`.
//!
//! The rule refuses credit only when the assertion calls the owner and every
//! such call sits under a coarse projection. A direct operand, or an owner
//! call that also appears bare elsewhere in the assertion, keeps its verdict.
//! `len(...)` over an owner result is coarse only through a closed allow-list:
//! `len(str|repr(<owner mention>))` measures text, and `len(<owner call>)`
//! (optionally wrapped in `list`/`tuple`/`sorted`/`reversed`) counts elements
//! that a changed `yield` or comprehension element (`element_only_change`)
//! leaves as they were. Any other shape (`len(",".join(..))`, `len(f(3)[0])`,
//! `len(f(3).attr)`, `len(set(..))`) can change length and keeps its verdict.
//! Only the operands of a top-level `assert` (or the sole argument of
//! `assertTrue`/`assertFalse`) are the asserted values; a comparison there
//! is a genuine pin. A comparison passed as a value (`assertEqual(f(2) > 0,
//! True)`) is a coarse projection. `%` with a string left operand is string
//! formatting, not a remainder.

use super::source_facts::parse_module_result;
use super::{expr_full_name, source_utils::text_for_range};
use rustpython_parser::ast::{self, Expr, Mod, Ranged, Stmt};
use std::path::Path;

/// Wrappers that re-materialize their one argument keeping every element, so
/// the count equals the count of the wrapped owner result. `set` and
/// `frozenset` are deliberately absent: they dedupe, so an element-only edit
/// can change the count.
const MATERIALIZERS: &[&str] = &["list", "tuple", "sorted", "reversed"];

/// Whether the edit from `old` to `new` replaces only the element expression
/// of a `yield` or a comprehension, so the number of produced elements is
/// unchanged. `false` when the old line is unknown.
pub(super) fn element_only_change(old: Option<&str>, new: &str) -> bool {
    let Some(old) = old else {
        return false;
    };
    let (old, new) = (old.trim(), new.trim());
    if old == new {
        return false;
    }
    let plain_yield = |line: &str| {
        line.strip_prefix("yield ")
            .is_some_and(|rest| !rest.starts_with("from "))
    };
    if plain_yield(old) && plain_yield(new) {
        return true;
    }
    // A comprehension whose `for`/`if` clauses are untouched.
    match (old.find(" for "), new.find(" for ")) {
        (Some(old_at), Some(new_at)) => {
            old[old_at..] == new[new_at..]
                && old[..old_at].contains(['[', '(', '{'])
                && new[..new_at].contains(['[', '(', '{'])
        }
        _ => false,
    }
}

/// True when the assertion `text` calls one of `names` and every such call is
/// read only through a coarse projection of its result.
pub(super) fn owner_call_only_in_coarse_projection(
    text: &str,
    names: &[String],
    element_only: bool,
) -> bool {
    let source = text.trim();
    let Ok(Mod::Module(module)) = parse_module_result(Path::new("<assertion>"), source) else {
        return false;
    };
    let [stmt] = module.body.as_slice() else {
        return false;
    };
    let mut walk = Walk {
        source,
        names,
        element_only,
        precise: 0,
        coarse: 0,
    };
    match stmt {
        Stmt::Assert(assert_stmt) => walk.operands(assert_stmt.test.as_ref()),
        Stmt::Expr(expr_stmt) => match expr_stmt.value.as_ref() {
            Expr::Call(call) => {
                let asserts_truth = expr_full_name(call.func.as_ref()).is_some_and(|name| {
                    matches!(
                        name.rsplit('.').next().unwrap_or(&name),
                        "assertTrue" | "assertFalse"
                    )
                });
                for argument in &call.args {
                    if asserts_truth {
                        walk.operands(argument);
                    } else {
                        walk.visit(argument, false);
                    }
                }
                for keyword in &call.keywords {
                    walk.visit(&keyword.value, false);
                }
            }
            other => walk.visit(other, false),
        },
        _ => return false,
    }
    walk.coarse > 0 && walk.precise == 0
}

struct Walk<'a> {
    source: &'a str,
    names: &'a [String],
    element_only: bool,
    precise: usize,
    coarse: usize,
}

impl Walk<'_> {
    /// The operands of a top-level comparison are the asserted values.
    fn operands(&mut self, test: &Expr) {
        match test {
            Expr::Compare(compare) => {
                self.visit(compare.left.as_ref(), false);
                for comparator in &compare.comparators {
                    self.visit(comparator, false);
                }
            }
            // Each conjunct of `assert a and b` is asserted on its own, and
            // `assert not a == b` asserts the comparison's operands.
            Expr::BoolOp(bool_op) if bool_op.op == ast::BoolOp::And => {
                bool_op.values.iter().for_each(|value| self.operands(value));
            }
            Expr::UnaryOp(unary) if unary.op == ast::UnaryOp::Not => {
                self.operands(unary.operand.as_ref());
            }
            other => self.visit(other, false),
        }
    }

    fn is_owner_call(&self, call: &ast::ExprCall) -> bool {
        expr_full_name(call.func.as_ref()).is_some_and(|name| {
            self.names.iter().any(|owner| {
                !owner.is_empty() && (name == *owner || name.ends_with(&format!(".{owner}")))
            })
        })
    }

    fn mentions_owner(&self, expr: &Expr) -> bool {
        let text = text_for_range(self.source, expr.range());
        self.names
            .iter()
            .any(|name| !name.is_empty() && super::mentions_python_name(&text, name))
    }

    fn generators(&mut self, generators: &[ast::Comprehension], coarse: bool) {
        for generator in generators {
            self.visit(&generator.iter, coarse);
            generator
                .ifs
                .iter()
                .for_each(|cond| self.visit(cond, coarse));
        }
    }

    fn visit(&mut self, expr: &Expr, coarse: bool) {
        match expr {
            Expr::Call(call) => {
                if self.is_owner_call(call) {
                    if coarse {
                        self.coarse += 1;
                    } else {
                        self.precise += 1;
                    }
                }
                let callee = expr_full_name(call.func.as_ref());
                let name = callee
                    .as_deref()
                    .map(|name| name.rsplit('.').next().unwrap_or(name));
                let inner = match (name, call.args.as_slice()) {
                    // `len(str(f(x)))` measures text; `len(list(gen(n)))` of an
                    // element-only edit counts elements the edit did not change.
                    (Some("len"), [argument]) => coarse || self.len_is_coarse(argument),
                    _ => coarse,
                };
                self.visit(call.func.as_ref(), coarse);
                // `all(f(x) == 6 for x in xs)` asserts each element's equality
                // just as a top-level `assert` does, so its operands are the
                // asserted values and a `==` there is a pin. Any other element
                // (an ordering, `is`, a bare value) is walked as a value.
                if !coarse && matches!(name, Some("all" | "any")) {
                    let equality = |element: &Expr| {
                        matches!(element, Expr::Compare(c) if c.ops.iter().all(|op|
                            matches!(op, ast::CmpOp::Eq | ast::CmpOp::NotEq)))
                    };
                    let element_and_generators = match call.args.as_slice() {
                        [Expr::GeneratorExp(comp)] => Some((&comp.elt, &comp.generators)),
                        [Expr::ListComp(comp)] => Some((&comp.elt, &comp.generators)),
                        _ => None,
                    };
                    if let Some((element, generators)) =
                        element_and_generators.filter(|(element, _)| equality(element))
                    {
                        self.operands(element.as_ref());
                        self.generators(generators, false);
                        return;
                    }
                }
                for argument in &call.args {
                    self.visit(argument, inner);
                }
                for keyword in &call.keywords {
                    self.visit(&keyword.value, inner);
                }
            }
            // A comparison reached here is consumed as a value (compared again
            // or passed to `assertEqual`), so only its boolean outcome is read.
            // Asserted comparisons go through `operands` instead.
            Expr::Compare(compare) => {
                self.visit(compare.left.as_ref(), true);
                for comparator in &compare.comparators {
                    self.visit(comparator, true);
                }
            }
            // `a or b` yields one of its operands, so it keeps their reading.
            Expr::BoolOp(bool_op) => bool_op.values.iter().for_each(|v| self.visit(v, coarse)),
            Expr::UnaryOp(unary) => self.visit(
                unary.operand.as_ref(),
                coarse || matches!(unary.op, ast::UnaryOp::Not),
            ),
            Expr::BinOp(bin_op) => {
                // `"fmt %s" % f(x)` is string formatting, not a remainder: only a
                // left operand that is not a string literal reads as a residue.
                let remainder = matches!(bin_op.op, ast::Operator::Mod)
                    && !matches!(
                        bin_op.left.as_ref(),
                        Expr::JoinedStr(_)
                            | Expr::Constant(ast::ExprConstant {
                                value: ast::Constant::Str(_) | ast::Constant::Bytes(_),
                                ..
                            })
                    );
                self.visit(bin_op.left.as_ref(), coarse || remainder);
                self.visit(bin_op.right.as_ref(), coarse);
            }
            Expr::Attribute(attribute) => {
                let coarse = coarse || matches!(attribute.attr.as_str(), "__class__" | "__name__");
                self.visit(attribute.value.as_ref(), coarse);
            }
            Expr::Subscript(subscript) => {
                self.visit(subscript.value.as_ref(), coarse);
                self.visit(subscript.slice.as_ref(), coarse);
            }
            Expr::Tuple(tuple) => tuple.elts.iter().for_each(|e| self.visit(e, coarse)),
            Expr::List(list) => list.elts.iter().for_each(|e| self.visit(e, coarse)),
            Expr::Starred(starred) => self.visit(starred.value.as_ref(), coarse),
            Expr::Await(await_expr) => self.visit(await_expr.value.as_ref(), coarse),
            Expr::Set(set) => set.elts.iter().for_each(|e| self.visit(e, coarse)),
            Expr::Dict(dict) => {
                dict.keys
                    .iter()
                    .flatten()
                    .for_each(|k| self.visit(k, coarse));
                dict.values.iter().for_each(|v| self.visit(v, coarse));
            }
            Expr::GeneratorExp(comp) => {
                self.visit(comp.elt.as_ref(), coarse);
                self.generators(&comp.generators, coarse);
            }
            Expr::ListComp(comp) => {
                self.visit(comp.elt.as_ref(), coarse);
                self.generators(&comp.generators, coarse);
            }
            Expr::SetComp(comp) => {
                self.visit(comp.elt.as_ref(), coarse);
                self.generators(&comp.generators, coarse);
            }
            Expr::DictComp(comp) => {
                self.visit(comp.key.as_ref(), coarse);
                self.visit(comp.value.as_ref(), coarse);
                self.generators(&comp.generators, coarse);
            }
            // `a if c else b` yields one branch, so it keeps their reading; the
            // condition only selects the branch and is read as a value.
            Expr::IfExp(if_exp) => {
                self.visit(if_exp.test.as_ref(), coarse);
                self.visit(if_exp.body.as_ref(), coarse);
                self.visit(if_exp.orelse.as_ref(), coarse);
            }
            Expr::Lambda(lambda) => self.visit(lambda.body.as_ref(), coarse),
            Expr::Name(_) | Expr::Constant(_) => {}
            // A shape the walk still does not follow: an owner mention there is
            // a read it cannot place. Demotion needs proof that every owner
            // call is coarse, so it counts as precise and the verdict stays.
            other => {
                if self.mentions_owner(other) {
                    self.precise += 1;
                }
            }
        }
    }

    /// Whether `len(argument)` reads a length the edit leaves unchanged: text
    /// length of an owner result, or the element count of a bare owner call
    /// (through count-preserving materializers) under an element-only edit.
    fn len_is_coarse(&self, argument: &Expr) -> bool {
        let mut inner = argument;
        while let Expr::Call(call) = inner {
            let name = expr_full_name(call.func.as_ref());
            let name = name.as_deref().map(|n| n.rsplit('.').next().unwrap_or(n));
            match (name, call.args.as_slice()) {
                (Some("str" | "repr"), [_]) => return self.mentions_owner(inner),
                (Some(wrapper), [only]) if MATERIALIZERS.contains(&wrapper) => inner = only,
                _ => break,
            }
        }
        self.element_only && matches!(inner, Expr::Call(call) if self.is_owner_call(call))
    }
}

#[cfg(test)]
mod tests {
    use super::{element_only_change, owner_call_only_in_coarse_projection};

    fn coarse(text: &str, element_only: bool) -> bool {
        owner_call_only_in_coarse_projection(text, &["f".to_string()], element_only)
    }

    #[test]
    fn comparison_residue_class_and_text_length_projections_are_coarse() {
        for text in [
            "self.assertEqual(f(2) is None, False)",
            "self.assertEqual(f(2) > 0, True)",
            "assert (f(2) > 0) == True",
            "assert f(3) % 3 == 0",
            "self.assertEqual(f(2).__class__.__name__, 'generator')",
            "self.assertEqual(f(2).__name__, 'int')",
            "assert len(str(f(3))) == 1",
            "self.assertEqual(len(repr(f(2))), 1)",
        ] {
            assert!(coarse(text, false), "{text}");
        }
    }

    #[test]
    fn exact_boundary_and_transparent_reads_keep_credit() {
        for text in [
            "assert f(3) == 6",
            "assert f(3) > 5",
            "self.assertEqual(f(3), 6)",
            "assert str(f(3)) == '6'",
            "assert f(3) + 1 == 7",
            "assert f(3) % 4 == 1 and f(3) == 6",
            "assert not f(3) == 6",
            "assert len(f(3)) == 3",
            "assert len(list(f(3))) == 3",
            "assert g(2) % 3 == 0",
            "assert f(2) > 0 == f(2)",
        ] {
            assert!(!coarse(text, false), "{text}");
        }
    }

    #[test]
    fn a_direct_call_beside_a_coarse_one_keeps_credit() {
        assert!(!coarse("assert f(3) == 6 and f(3) % 3 == 0", false));
        assert!(!coarse("self.assertEqual(f(3), f(2) > 0)", false));
    }

    #[test]
    fn length_of_an_element_only_edit_is_coarse() {
        assert!(coarse("assert len(list(f(3))) == 3", true));
        assert!(coarse("self.assertEqual(len(sorted(f(3))), 3)", true));
        assert!(coarse("assert len(f(3)) == 3", true));
        // A call that is not the owner is not measured.
        assert!(!coarse("assert len(list(g(3))) == 3", true));
        // The value itself is still observed.
        assert!(!coarse("assert list(f(3)) == [0, 3, 6]", true));
    }

    #[test]
    fn asserted_comparisons_are_pins_even_through_assert_true() {
        for text in [
            "self.assertTrue(f(3) == 6)",
            "self.assertTrue(f(3) > 5)",
            "self.assertFalse(f(3) != 6)",
            "self.assertTrue(f(3) == 6 and g(1))",
        ] {
            assert!(!coarse(text, false), "{text}");
        }
        // The same comparison passed as a value is still a coarse projection.
        assert!(coarse("self.assertEqual(f(3) > 5, True)", false));
    }

    #[test]
    fn string_formatting_is_not_a_remainder() {
        assert!(!coarse("assert \"total=%s\" % f(3) == \"total=6\"", false));
        assert!(!coarse("assert b\"%d\" % f(3) == b\"6\"", false));
        assert!(coarse("assert f(3) % 3 == 0", false));
    }

    #[test]
    fn length_outside_the_allow_list_keeps_credit() {
        for text in [
            "assert len(set(f(3))) == 3",
            "assert len(frozenset(f(3))) == 3",
            "assert len(\",\".join(f(3))) == 5",
            "assert len(f(3)[0]) == 3",
            "assert len(f(3).attr) == 3",
        ] {
            assert!(!coarse(text, true), "{text}");
        }
    }

    #[test]
    fn comprehension_set_dict_and_ternary_shapes_are_followed() {
        for text in [
            "assert all(f(2) > 0 for _ in xs)",
            "assert any([f(2) is None for _ in xs])",
            "assert {f(2) > 0 for _ in xs} == {True}",
            "assert {k: f(2) > 0 for k in xs} == {}",
            "assert {f(2) > 0, True} == {True}",
            "assert {1: f(2) > 0} == {1: True}",
            "self.assertEqual(f(2) > 0 if xs else False, True)",
            "assert all(f(x) % 3 == 0 for x in xs)",
            "assert sorted(map(lambda y: f(y) % 3, xs)) == [0]",
        ] {
            assert!(coarse(text, false), "{text}");
        }
    }

    #[test]
    fn precise_reads_inside_followed_shapes_keep_credit() {
        for text in [
            "assert [f(x) for x in xs] == [6]",
            "assert all(f(x) == 6 for x in xs)",
            "assert all(f(2) > 0 for _ in xs) and f(2) == 6",
            "assert {f(2): 1} == {6: 1}",
            "assert {f(2)} == {6}",
            "self.assertEqual(f(2) if xs else 0, 6)",
            "assert [f(x) % 3 for x in xs if f(x)] == [0]",
        ] {
            assert!(!coarse(text, false), "{text}");
        }
    }

    #[test]
    fn element_only_change_reads_yield_and_comprehension_edits() {
        assert!(element_only_change(
            Some("        yield i * 2"),
            "        yield i * 3"
        ));
        assert!(element_only_change(
            Some("    return [i * 2 for i in range(n) if i]"),
            "    return [i * 3 for i in range(n) if i]"
        ));
        assert!(!element_only_change(
            Some("    return [i for i in range(n) if i]"),
            "    return [i for i in range(n)]"
        ));
        assert!(!element_only_change(Some("yield from a"), "yield from b"));
        assert!(!element_only_change(None, "yield 3"));
        assert!(!element_only_change(Some("yield 3"), "yield 3"));
    }
}
