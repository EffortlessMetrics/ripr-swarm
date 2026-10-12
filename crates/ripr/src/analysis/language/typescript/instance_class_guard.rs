//! RIPR-SPEC-0249 rule 1: `toBeInstanceOf(C)` pins which branch of a
//! changed predicate ran when the owner returns a different class on each
//! side.
//!
//! `if (status === 404) return new NotFound(); return new Forbidden();` with
//! `expect(errorFor(404)).toBeInstanceOf(NotFound)` fails as soon as the
//! predicate sends 404 to the other return, because a `Forbidden` is not a
//! `NotFound`. The matcher alone is not an exact discriminator (it passes
//! for any instance of `C` or of a subclass), so it reads as one only for a
//! predicate change, only on the whole result of an owner call, and only
//! when every value the owner returns is a `new` of a class whose relation
//! to `C` the owner's module settles.

use super::*;

/// Classes the language provides. None of them can extend a class the
/// owner's module declares.
const BUILTIN_CLASSES: [&str; 17] = [
    "Error",
    "TypeError",
    "RangeError",
    "SyntaxError",
    "ReferenceError",
    "EvalError",
    "URIError",
    "AggregateError",
    "Object",
    "Array",
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
    "Date",
    "RegExp",
    "Promise",
];

/// The `toBeInstanceOf` assertions that read `exact_value` / strong for one
/// predicate change, by test file and assertion line.
#[derive(Debug, Default)]
pub(crate) struct InstanceClassFacts {
    credited: Vec<(PathBuf, usize)>,
}

impl InstanceClassFacts {
    /// `true` when `assertion` in the test at `file` is a credited
    /// `toBeInstanceOf` pin.
    pub(crate) fn credits(&self, file: &Path, assertion: &TypeScriptAssertion) -> bool {
        assertion.matcher == "toBeInstanceOf"
            && self
                .credited
                .iter()
                .any(|(path, line)| path == file && *line == assertion.line)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.credited.is_empty()
    }
}

/// The identifier that starts `text`, when one does.
fn leading_identifier(text: &str) -> Option<&str> {
    let end = text
        .char_indices()
        .find(|(_, c)| !(c.is_alphanumeric() || matches!(c, '_' | '$')))
        .map_or(text.len(), |(index, _)| index);
    let name = &text[..end];
    name.chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || matches!(c, '_' | '$'))
        .then_some(name)
}

/// The classes a module declares (`class Name`), each with its `extends`
/// clause as written (empty when there is none). `None` when a class
/// declaration has no name the scan can read, or a name repeats.
fn module_classes(masked_module: &str) -> Option<Vec<(String, String)>> {
    let mut classes: Vec<(String, String)> = Vec::new();
    for at in identifier_offsets(masked_module, "class") {
        let rest = masked_module[at + "class".len()..].trim_start();
        let name = leading_identifier(rest)?;
        if name == "extends" || classes.iter().any(|(known, _)| known == name) {
            return None;
        }
        let header = &rest[name.len()..];
        let header = &header[..header.find('{')?];
        // A generic header (`class A<T> extends B`) is not read.
        if header.trim_start().starts_with('<') {
            return None;
        }
        let parent = header
            .trim_start()
            .strip_prefix("extends")
            .map_or("", str::trim)
            .to_string();
        classes.push((name.to_string(), parent));
    }
    Some(classes)
}

/// The class each `return` of the owner body constructs (`return new X(...)`).
/// `None` when the body has a nested function (whose `return` is not the
/// owner's) or a `return` of anything else.
fn returned_classes(masked_body: &str) -> Option<Vec<String>> {
    if masked_body.contains("=>") || !identifier_offsets(masked_body, "function").is_empty() {
        return None;
    }
    let returns = identifier_offsets(masked_body, "return");
    if returns.is_empty() {
        return None;
    }
    returns
        .into_iter()
        .map(|at| {
            let rest = masked_body[at + "return".len()..].trim_start();
            let rest = rest.strip_prefix("new")?;
            if !rest.starts_with(char::is_whitespace) {
                return None;
            }
            let rest = rest.trim_start();
            let class = leading_identifier(rest)?;
            rest[class.len()..]
                .trim_start()
                .starts_with('(')
                .then(|| class.to_string())
        })
        .collect()
}

/// `true` when an instance of `class` can never pass `toBeInstanceOf(asserted)`:
/// its chain reaches a builtin or the top without meeting `asserted`, and
/// every class on the way is one the module declares.
fn never_instance_of(class: &str, asserted: &str, classes: &[(String, String)]) -> bool {
    let mut current = class;
    for _ in 0..=classes.len() {
        if current == asserted {
            return false;
        }
        if BUILTIN_CLASSES.contains(&current) {
            return true;
        }
        let Some((_, parent)) = classes.iter().find(|(name, _)| name == current) else {
            return false; // imported or unknown: its chain is not read
        };
        if parent.is_empty() {
            return true;
        }
        match leading_identifier(parent) {
            // `extends Base` only; `extends mixin(Base)` or `extends ns.Base`
            // is not read.
            Some(next) if next.len() == parent.len() => current = next,
            _ => return false,
        }
    }
    false
}

/// RIPR-SPEC-0249 rule 1's owner-side condition: the module declares
/// `asserted`, every `return` of the owner constructs a class, at least one
/// returns `asserted` and at least one returns a class that is never an
/// `asserted` instance, and no returned class could be one through a
/// subclass.
pub(crate) fn ts_owner_returns_distinct_classes(
    owner_source: &str,
    module_source: &str,
    asserted: &str,
) -> bool {
    let Some(body) = masked_owner_body(owner_source) else {
        return false;
    };
    let Some(returned) = returned_classes(&body) else {
        return false;
    };
    let module = ts_mask_literals_and_comments(module_source);
    let Some(classes) = module_classes(&module) else {
        return false;
    };
    if !classes.iter().any(|(name, _)| name == asserted) {
        return false;
    }
    let mut saw_asserted = false;
    let mut saw_other = false;
    for class in &returned {
        if class == asserted {
            saw_asserted = true;
        } else if never_instance_of(class, asserted, &classes) {
            saw_other = true;
        } else {
            return false;
        }
    }
    saw_asserted && saw_other
}

/// Collect the credited `toBeInstanceOf` assertions for a predicate change:
/// each on an oracle relation, on the whole result of one owner call
/// (`owner(...)`, `ns.owner(...)`, awaited, or a local bound once to that
/// call), naming a class the owner returns on some path and not on another.
pub(crate) fn ts_instance_class_facts(
    probe_family: &ProbeFamily,
    owner: &TypeScriptOwner,
    candidates: &[TypeScriptRelatedCandidate<'_>],
    alias_map: Option<&TsAliasMap>,
    workspace_root: Option<&Path>,
) -> InstanceClassFacts {
    let mut facts = InstanceClassFacts::default();
    if !matches!(probe_family, ProbeFamily::Predicate) {
        return facts;
    }
    let pins = |assertion: &TypeScriptAssertion| {
        assertion.matcher == "toBeInstanceOf" && assertion.expected_value_or_variant.is_some()
    };
    if !candidates
        .iter()
        .any(|candidate| candidate.test.assertions.iter().any(pins))
    {
        return facts;
    }
    let Some(owner_source) = owner.source_text.as_deref() else {
        return facts;
    };
    let Some(module_source) = workspace_root
        .and_then(|root| {
            crate::analysis::committed_source::read_source_bytes(root, &owner.file)
                .ok()
                .flatten()
        })
        .and_then(|bytes| String::from_utf8(bytes).ok())
    else {
        return facts;
    };
    let mut owner_side: Vec<(String, bool)> = Vec::new();
    for candidate in candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
    {
        if local_identifier_declared_in_test_body(&candidate.test.body_text, &owner.name) {
            continue;
        }
        let receivers = owner_namespace_receivers(candidate.test, owner, alias_map, workspace_root);
        for assertion in candidate.test.assertions.iter().filter(|a| pins(a)) {
            let Some(class) = assertion.expected_value_or_variant.as_deref() else {
                continue;
            };
            let owner_ok = match owner_side.iter().find(|(known, _)| known == class) {
                Some((_, ok)) => *ok,
                None => {
                    let ok = ts_owner_returns_distinct_classes(owner_source, &module_source, class);
                    owner_side.push((class.to_string(), ok));
                    ok
                }
            };
            if owner_ok
                && ts_observes_whole_owner_result(candidate.test, assertion, owner, &receivers)
            {
                facts
                    .credited
                    .push((candidate.test.file.clone(), assertion.line));
            }
        }
    }
    facts
}
