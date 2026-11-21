//! #11738: per-scan answers for the two module-level readings every test
//! of a file asks again.
//!
//! [`owner_pin_assertions`](super::owner_pin_assertions) admits each test
//! function of a file in turn. For each, `#[test]` asks whether the
//! enclosing module rebinds the name `test`
//! ([`builtin_name_holds`](super::builtin_name_holds): every `use` and
//! `extern crate` of the module), and the item-context check reads the
//! attributes of every enclosing item, the source file included, by walking
//! all of that node's children. Both answers depend on the module node
//! alone, yet were recomputed per test: a file of N tests in one module
//! walked N items N times (8,000 tests in one file spent most of the
//! remaining run there).
//!
//! Within one scan both are functions of the syntax node (and the name), so
//! the scan keeps them here. As with
//! [`ExternCrateScan`](super::ExternCrateScan), the memo lives only while
//! the outermost [`ModuleScan`] guard on this thread does, and each entry
//! holds its node, which keeps that tree alive: a node of a later tree can
//! never compare equal to a stale entry through a reused address. Outside a
//! scan nothing is kept and every question is computed directly.

use ra_ap_syntax::{SyntaxNode, ast};
use std::cell::RefCell;
use std::collections::HashMap;

/// One scan's answers.
#[derive(Default)]
struct ModuleAnswers {
    /// [`super::builtin_name_holds`] by (module node, name).
    builtin_names: HashMap<(SyntaxNode, String), bool>,
    /// Per node, its child attributes whose test-build availability is not
    /// established as on, in child order.
    unsettled_attributes: HashMap<SyntaxNode, Vec<ast::Attr>>,
}

thread_local! {
    static MODULE_ANSWERS: RefCell<Option<ModuleAnswers>> = const { RefCell::new(None) };
}

#[cfg(test)]
thread_local! {
    /// Test-only: how many uncached module readings ran on this thread.
    pub(super) static MODULE_WALKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Keeps this thread's module memo alive while held; the outermost guard
/// clears it on drop.
pub(super) struct ModuleScan {
    outermost: bool,
}

impl ModuleScan {
    pub(super) fn enter() -> Self {
        MODULE_ANSWERS.with(|answers| {
            let mut answers = answers.borrow_mut();
            let outermost = answers.is_none();
            if outermost {
                *answers = Some(ModuleAnswers::default());
            }
            Self { outermost }
        })
    }
}

impl Drop for ModuleScan {
    fn drop(&mut self) {
        if self.outermost {
            MODULE_ANSWERS.with(|answers| answers.borrow_mut().take());
        }
    }
}

#[cfg(test)]
pub(super) fn module_memo_is_live() -> bool {
    MODULE_ANSWERS.with(|answers| answers.borrow().is_some())
}

#[cfg(test)]
fn count_walk() {
    MODULE_WALKS.with(|walks| walks.set(walks.get() + 1));
}

/// [`super::builtin_name_holds_uncached`], memoized within a scan.
pub(super) fn builtin_name_holds(name: &str, module: &SyntaxNode) -> bool {
    let key = (module.clone(), name.to_string());
    let known = MODULE_ANSWERS.with(|answers| {
        answers
            .borrow()
            .as_ref()
            .and_then(|answers| answers.builtin_names.get(&key).copied())
    });
    if let Some(known) = known {
        return known;
    }
    #[cfg(test)]
    count_walk();
    // Computed with no borrow held: the reading recurses into enclosing
    // modules through this function.
    let holds = super::builtin_name_holds_uncached(name, module);
    MODULE_ANSWERS.with(|answers| {
        if let Some(answers) = answers.borrow_mut().as_mut() {
            answers.builtin_names.insert(key, holds);
        }
    });
    holds
}

/// The child attributes of `node` whose test-build availability is not
/// established as on (`attribute_test_build_availability != Some(true)`),
/// in child order; memoized within a scan.
pub(super) fn unsettled_attributes(node: &SyntaxNode) -> Vec<ast::Attr> {
    let known = MODULE_ANSWERS.with(|answers| {
        answers
            .borrow()
            .as_ref()
            .and_then(|answers| answers.unsettled_attributes.get(node).cloned())
    });
    if let Some(known) = known {
        return known;
    }
    #[cfg(test)]
    count_walk();
    let attributes = unsettled_attributes_uncached(node);
    MODULE_ANSWERS.with(|answers| {
        if let Some(answers) = answers.borrow_mut().as_mut() {
            answers
                .unsettled_attributes
                .insert(node.clone(), attributes.clone());
        }
    });
    attributes
}

fn unsettled_attributes_uncached(node: &SyntaxNode) -> Vec<ast::Attr> {
    use ra_ap_syntax::AstNode;
    node.children()
        .filter_map(ast::Attr::cast)
        .filter(|attr| {
            crate::analysis::facts::cfg_predicates::attribute_test_build_availability(
                &attr.syntax().text().to_string(),
            ) != Some(true)
        })
        .collect()
}

#[cfg(test)]
mod tests;
