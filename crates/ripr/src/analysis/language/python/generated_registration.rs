//! Source-bound Hypothesis `.TestCase` registrations (#10761).
//!
//! These are discovery limitations, never executable tests or rule oracles.
use super::module_constants::ModuleBindingFacts;
use super::owners_tests::collect_imports_from_statements;
use super::related_tests::owner_module_paths;
use super::source_facts::{PythonSourceLimitation, parse_module_result};
use super::source_utils::{SourceText, line_for_range_start, normalized_path};
use super::{PythonImport, expr_full_name};
use crate::domain::StaticLimitKind;
use rustpython_parser::ast::{Expr, Mod, Ranged, Stmt};
use rustpython_parser::{Mode, Tok, lexer::lex};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct GeneratedRegistration {
    pub(super) name: String,
    pub(super) line: usize,
    pub(super) imports: Vec<PythonImport>,
}

impl GeneratedRegistration {
    pub(super) fn detail(&self, file: &Path) -> String {
        format!(
            "hypothesis_generated_registration: {}:{} `{}` retains one generated registration; native collected/executed counts are unknown. Selection, activation and eligible rules are unresolved; state-machine semantics are not analyzed. No extracted assertions does not mean no assertions. No rule/invariant credit, generated inputs, steps or executable selector is inferred.",
            normalized_path(file),
            self.line,
            self.name
        )
    }

    pub(super) fn limitation(&self, file: &Path) -> PythonSourceLimitation {
        PythonSourceLimitation {
            kind: StaticLimitKind::PropertyBasedTest,
            evidence: self.detail(file),
            missing: "Generated Hypothesis state-machine semantics require independent native collection and execution; no extracted assertions does not mean the generated test has no assertions.".to_string(),
        }
    }
}

pub(super) fn collect(
    file: &Path,
    source: &SourceText<'_>,
    body: &[Stmt],
    imports: &[PythonImport],
) -> Vec<GeneratedRegistration> {
    if !body.iter().any(|stmt| matches!(stmt, Stmt::Assign(assign)
        if matches!(assign.value.as_ref(), Expr::Attribute(value) if value.attr.as_str() == "TestCase"))) {
        return Vec::new();
    }
    let bindings = ModuleBindingFacts::of(source, body);
    let mut machines = BTreeSet::new();
    let mut registrations = Vec::new();
    let mut visible_imports = Vec::new();
    for stmt in body {
        if !matches!(stmt, Stmt::ImportFrom(import) if import.level.as_ref().is_some_and(|level| level.to_usize() > 0))
        {
            visible_imports.extend(collect_imports_from_statements(
                file,
                std::slice::from_ref(stmt),
            ));
        }
        if let Stmt::ClassDef(class) = stmt
            && class.bases.len() == 1
            && class.keywords.is_empty()
            && class.decorator_list.is_empty()
            && super::module_constants::class_scope_names(&class.body)
                .is_some_and(|names| !names.iter().any(|name| name == "TestCase"))
            && bindings.bound_once(class.name.as_str())
            && !bindings.may_assign_attribute("TestCase")
            && framework_base(&class.bases[0], &visible_imports, &bindings)
        {
            machines.insert(class.name.to_string());
        }
        let Stmt::Assign(assign) = stmt else { continue };
        let [Expr::Name(target)] = assign.targets.as_slice() else {
            continue;
        };
        let Expr::Attribute(value) = assign.value.as_ref() else {
            continue;
        };
        let Expr::Name(machine) = value.value.as_ref() else {
            continue;
        };
        if value.attr.as_str() == "TestCase"
            && target.id.starts_with("Test")
            && bindings.bound_once(target.id.as_str())
            && machines.contains(machine.id.as_str())
        {
            registrations.push(GeneratedRegistration {
                name: target.id.to_string(),
                line: line_for_range_start(source, stmt.range()),
                imports: imports.to_vec(),
            });
        }
    }
    registrations
}

fn framework_base(expr: &Expr, imports: &[PythonImport], bindings: &ModuleBindingFacts) -> bool {
    // A stable module name does not make its members immutable. Dunder
    // writes can also replace a class's inheritance or lookup machinery.
    if bindings.may_assign_attribute("RuleBasedStateMachine")
        || bindings.may_assign_attribute("stateful")
        || bindings.assigns_dunder()
    {
        return false;
    }
    // expr_full_name also unwraps calls for decorator consumers. A class
    // base here must instead be a direct name/attribute chain, not a factory.
    let mut root = expr;
    while let Expr::Attribute(attribute) = root {
        root = attribute.value.as_ref();
    }
    if !matches!(root, Expr::Name(_)) {
        return false;
    }
    let Some(name) = expr_full_name(expr) else {
        return false;
    };
    imports.iter().any(|import| {
        let root = import.alias.split('.').next().unwrap_or_default();
        if !bindings.bound_once(root) {
            return false;
        }
        (import.source_module == "hypothesis.stateful"
            && import.imported == "RuleBasedStateMachine"
            && name == import.alias)
            || (import.source_module == "hypothesis"
                && import.imported == "stateful"
                && name == format!("{}.RuleBasedStateMachine", import.alias))
            || (import.source_module.is_empty()
                && (import.imported == "hypothesis.stateful"
                    && name == format!("{}.RuleBasedStateMachine", import.alias)))
    })
}

/// Disclose a relevant registration or an unresolved import closure. A closed
/// workspace import graph disjoint from the changed paths is demonstrably
/// unrelated. Failure to parse/resolve, cycles beyond the bound and external
/// imports stay unknown. This grants no positive reach or oracle evidence.
pub(super) fn relevant(
    file: &Path,
    registration: &GeneratedRegistration,
    changed: &[PathBuf],
    sources: &HashMap<PathBuf, String>,
) -> bool {
    if changed.is_empty() {
        return false;
    }
    let mut pending = vec![(file.to_path_buf(), registration.imports.clone())];
    let mut seen = BTreeSet::new();
    while let Some((path, imports)) = pending.pop() {
        if changed.contains(&path) {
            return true;
        }
        let Some(source) = sources.get(&path) else {
            return true;
        };
        let Ok(Mod::Module(module)) = parse_module_result(&path, source) else {
            return true;
        };
        let mut ranges = Vec::new();
        super::source_facts::collect_import_line_ranges(
            &SourceText::new(source),
            &module.body,
            &mut ranges,
        );
        if ranges.len()
            > module
                .body
                .iter()
                .filter(|stmt| matches!(stmt, Stmt::Import(_) | Stmt::ImportFrom(_)))
                .count()
            || super::module_constants::writes_namespace_dynamically(source)
            || lex(source, Mode::Module).any(|token| match token {
                Ok((Tok::Name { name }, _)) => name == "__import__",
                // An f-string can execute arbitrary expressions. Its text is
                // not an ordinary literal proving a closed import graph.
                Ok((Tok::String { kind, .. }, _)) => kind.is_any_fstring(),
                Err(_) => true,
                _ => false,
            })
        {
            return true;
        }
        if !seen.insert(path) {
            continue;
        }
        if seen.len() > 64 {
            return true;
        }
        for import in imports {
            let module = if import.source_module.is_empty() {
                &import.imported
            } else {
                &import.source_module
            };
            if ["hypothesis", "pytest", "unittest"]
                .iter()
                .any(|framework| {
                    module == framework || module.starts_with(&format!("{framework}."))
                })
            {
                continue;
            }
            let matches: Vec<_> = sources
                .iter()
                .filter(|(path, _)| {
                    owner_module_paths(path).iter().any(|candidate| {
                        candidate == module || candidate == &format!("{module}.{}", import.imported)
                    })
                })
                .collect();
            if matches.is_empty() {
                return true;
            }
            for (path, source) in matches {
                if changed.contains(path) {
                    return true;
                }
                if seen.contains(path) {
                    continue;
                }
                let Ok(Mod::Module(module)) = parse_module_result(path, source) else {
                    return true;
                };
                pending.push((
                    path.clone(),
                    collect_imports_from_statements(path, &module.body),
                ));
            }
        }
    }
    false
}
