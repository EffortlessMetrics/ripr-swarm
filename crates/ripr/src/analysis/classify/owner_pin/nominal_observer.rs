//! Index-owned nominal dispatch and enum equality decisions. These do not
//! identify an assertion, receiver, field or getter body by themselves.
use super::*;

type Generation = (usize, usize, u64);

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
enum Decision {
    Dispatch {
        id: String,
        file: PathBuf,
        name: String,
        start: usize,
        end: usize,
        self_type: String,
        body: crate::analysis::facts::SourceText,
    },
    EnumEquality {
        qualifier: String,
        package_names: BTreeSet<String>,
    },
}

/// Owned by CallIdentityMemos. A clone starts empty; file changes clear it.
/// Generation also catches flat fact mutation and test-only source mutation.
#[derive(Default)]
pub(in crate::analysis::classify) struct NominalObserverMemo {
    generation: Option<Generation>,
    decisions: BTreeMap<Decision, bool>,
}

fn verdict(index: &RustIndex, key: Decision, compute: impl FnOnce() -> bool) -> bool {
    // Warm hits must retain both captured universes in the diagnostic read
    // trace. This is not a sealed dependency ledger or a cross-index cache.
    use crate::analysis::probe_read_trace::{Universe, universe};
    universe(Universe::Files);
    universe(Universe::Functions);
    let generation = index.storage_identity();
    if generation.2 == u64::MAX {
        return compute();
    }
    let memo = &index.call_identity_memos.nominal_observers;
    {
        let mut memo = memo.borrow_mut();
        if memo.generation != Some(generation) {
            *memo = NominalObserverMemo {
                generation: Some(generation),
                ..Default::default()
            };
        }
        if let Some(answer) = memo.decisions.get(&key) {
            return *answer;
        }
    }
    // No guard survives a source read or another authority's computation.
    // Concurrent cold misses can compute twice; sequential hits do no scans.
    let answer = compute();
    let mut memo = memo.borrow_mut();
    if memo.generation == Some(generation) {
        memo.decisions.insert(key, answer);
    }
    answer
}

#[cfg(test)]
thread_local! {
    static COMPUTATIONS: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, 0)) };
}

pub(super) fn dispatch(owner: &FunctionSummary, index: &RustIndex) -> bool {
    let FunctionContainer::Inherent { self_ty } = &owner.item.container else {
        return false;
    };
    let key = Decision::Dispatch {
        id: owner.id.0.clone(),
        file: owner.file.clone(),
        name: owner.name.clone(),
        start: owner.start_line,
        end: owner.end_line,
        self_type: self_ty.clone(),
        body: owner.body.clone(),
    };
    verdict(index, key, || dispatch_uncached(owner, index))
}

fn dispatch_uncached(owner: &FunctionSummary, index: &RustIndex) -> bool {
    #[cfg(test)]
    COMPUTATIONS.with(|count| {
        let (dispatch, equality) = count.get();
        count.set((dispatch + 1, equality));
    });
    !other_definition_competes(owner, index, CallKind::Method, None, false)
}

pub(super) fn enum_equality(qualifier: &str, index: &RustIndex) -> bool {
    let key = Decision::EnumEquality {
        qualifier: qualifier.to_string(),
        package_names: index.package_names.clone(),
    };
    verdict(index, key, || enum_equality_uncached(qualifier, index))
}

fn enum_equality_uncached(qualifier: &str, index: &RustIndex) -> bool {
    #[cfg(test)]
    COMPUTATIONS.with(|count| {
        let (dispatch, equality) = count.get();
        count.set((dispatch, equality + 1));
    });
    derived_equality(qualifier, index).is_some_and(|(facts, declaring)| {
        facts.fields.iter().all(|field| {
            !field.has_attributes && type_compares_by_value(&field.ty, declaring, index, 0)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIB: &str = "src/lib.rs";
    const OWNER: &str = "pub struct Counter { pub count: u32, pub other: u32 }\nimpl Counter {\n pub fn bump(&mut self) { self.count += 1; }\n pub fn read(&self) -> u32 { self.count }\n}\n";
    const ENUM: &str = "#[derive(Debug, PartialEq, Eq)]\npub enum State { Idle, Done }\n";

    fn index(source: &str) -> RustIndex {
        let mut index = RustIndex::default();
        replace(&mut index, LIB, source);
        index
    }

    fn replace(index: &mut RustIndex, path: &str, source: &str) {
        let path = PathBuf::from(path);
        let _ = index.remove_file(&path);
        let facts = crate::analysis::rust_index::summarize_file(path.clone(), source.to_string());
        index.insert_file(path, facts, true);
    }

    fn owner(index: &RustIndex, name: &str) -> Result<FunctionSummary, String> {
        index
            .functions()
            .iter()
            .find(|owner| owner.name == name)
            .cloned()
            .ok_or_else(|| format!("missing owner {name}"))
    }

    fn counts() -> (usize, usize) {
        COMPUTATIONS.with(std::cell::Cell::get)
    }

    #[test]
    fn nominal_observer_dispatch_reuses_work_and_exact_owner_keys() -> Result<(), String> {
        let index = index(OWNER);
        let bump = owner(&index, "bump")?;
        let read = owner(&index, "read")?;
        let before = counts();
        for _ in 0..32 {
            assert!(getter_dispatch_is_unambiguous(&bump, &index));
        }
        assert_eq!(
            counts().0 - before.0,
            1,
            "32 observations must share one dispatch scan"
        );
        assert!(getter_dispatch_is_unambiguous(&read, &index));
        assert_eq!(
            counts().0 - before.0,
            2,
            "distinct owner identities cannot share the entry"
        );
        let mut other_definition = bump.clone();
        other_definition.file = PathBuf::from("src/elsewhere.rs");
        other_definition.start_line = 99;
        other_definition.id.0.push_str("::elsewhere");
        assert!(
            !getter_dispatch_is_unambiguous(&other_definition, &index),
            "the same method name at another definition cannot borrow the owner's result"
        );
        assert_eq!(counts().0 - before.0, 3);
        let before = counts();
        for _ in 0..8 {
            assert!(dispatch_uncached(&bump, &index));
        }
        assert_eq!(
            counts().0 - before.0,
            8,
            "uncached removal control must perform eight scans"
        );
        Ok(())
    }

    #[test]
    fn nominal_observer_dispatch_invalidates_rivals_and_getter_bodies() -> Result<(), String> {
        let mut index = index(OWNER);
        let bump = owner(&index, "bump")?;
        assert!(getter_dispatch_is_unambiguous(&bump, &index));
        let rival =
            "trait Rival { fn bump(&self); }\nimpl Rival for Counter { fn bump(&self) {} }\n";
        replace(&mut index, "src/rival.rs", rival);
        assert!(!getter_dispatch_is_unambiguous(&bump, &index));
        assert!(index.remove_file(Path::new("src/rival.rs")));
        assert!(getter_dispatch_is_unambiguous(&bump, &index));
        // Flat membership changes need generation invalidation even without a
        // CallIdentityMemos::clear or a captured-file insertion.
        let facts = crate::analysis::rust_index::summarize_file(
            PathBuf::from("src/flat.rs"),
            rival.to_string(),
        );
        let rival = facts
            .functions
            .into_iter()
            .find(|function| function.name == "bump" && function.item.has_body)
            .ok_or("missing rival")?;
        let mut flat = index.clone();
        assert!(getter_dispatch_is_unambiguous(&bump, &flat));
        flat.push_function(rival);
        assert!(!getter_dispatch_is_unambiguous(&bump, &flat));
        for (source, expected) in [
            (OWNER.to_string(), true),
            (OWNER.replace("{ self.count }", "{ self.other }"), false),
            (OWNER.to_string(), true),
        ] {
            replace(&mut index, LIB, &source);
            let getter = owner(&index, "read")?;
            assert!(getter_dispatch_is_unambiguous(&getter, &index));
            assert_eq!(
                super::super::super::builder_override::getter_plainly_reads(
                    &index, "Counter", "read", "count"
                ),
                expected
            );
        }
        Ok(())
    }

    #[test]
    fn nominal_observer_enum_reuses_work_and_invalidates_equality() {
        let mut index = index(ENUM);
        let before = counts();
        for _ in 0..32 {
            assert!(error_enum_compares_payload("State", &index));
        }
        assert_eq!(
            counts().1 - before.1,
            1,
            "32 enum reads must share one equality scan"
        );
        let before = counts();
        for _ in 0..8 {
            assert!(enum_equality_uncached("State", &index));
        }
        assert_eq!(
            counts().1 - before.1,
            8,
            "uncached removal control must repeat the equality work"
        );
        let manual = ENUM.replace("Debug, PartialEq, Eq", "Debug")
            + "impl PartialEq for State { fn eq(&self, _: &Self) -> bool { true } }\n";
        replace(&mut index, LIB, &manual);
        assert!(!error_enum_compares_payload("State", &index));
        replace(&mut index, LIB, ENUM);
        assert!(error_enum_compares_payload("State", &index));
        replace(
            &mut index,
            "src/equality.rs",
            "impl PartialEq for State { fn eq(&self, _: &Self) -> bool { true } }",
        );
        assert!(!error_enum_compares_payload("State", &index));
        assert!(index.remove_file(Path::new("src/equality.rs")));
        assert!(error_enum_compares_payload("State", &index));
    }

    #[test]
    fn nominal_observer_enum_scalar_authority_and_refusals_are_not_shared() {
        let mut index = index(&("use own::*;\n".to_string() + ENUM));
        assert!(!error_enum_compares_payload("State", &index));
        index.package_names.insert("own".to_string());
        assert!(error_enum_compares_payload("State", &index));
        index.package_names.clear();
        assert!(!error_enum_compares_payload("State", &index));
    }

    #[test]
    fn nominal_observer_cached_dispatch_keeps_trait_generic_and_unindexed_refusals()
    -> Result<(), String> {
        for rival in [
            "trait Rival { fn bump(self); } impl Rival for Counter { fn bump(self) {} }",
            "trait Rival { fn bump(&self); } impl<T> Rival for T { fn bump(&self) {} }",
            "macro_rules! generated { () => { impl Counter { fn bump(&mut self) {} } }; }",
        ] {
            let index = index(&(OWNER.to_string() + rival));
            let owner = owner(&index, "bump")?;
            assert!(
                !dispatch_uncached(&owner, &index),
                "fixture must establish the competing definition"
            );
            let before = counts();
            for _ in 0..16 {
                assert!(!getter_dispatch_is_unambiguous(&owner, &index));
            }
            assert_eq!(
                counts().0 - before.0,
                1,
                "refusals are reused without weakening their decision"
            );
        }
        Ok(())
    }
}
