//! SPEC-0197 rule 7a: exact source/value binding, not oracle proximity.

use super::OwnerPinSyntax;
use crate::analysis::facts::{
    FunctionContainer, FunctionSourceRole, FunctionSummary, ModulePathTarget, OracleFact,
    RustIndex, SourceRoleProvenanceEdgeKind, TestSummary,
};
use crate::analysis::syntax::{decimal_u8, literal_check_syntax, literal_owner_syntax};
use crate::analysis::workspace::{
    AnyhowIdentity, config_snapshot_identity, verified_anyhow_identity,
};
use crate::domain::{Probe, ProbeFamily, SymbolId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::analysis) struct FallibleCheckReceipt {
    owner: SymbolId,
    helper: SymbolId,
    test: SymbolId,
    owner_value: u8,
    expected_value: u8,
    source_identity: String,
    source_file: PathBuf,
    source_line: usize,
    test_file: PathBuf,
    test_name: String,
    test_line: usize,
    assertion_line: usize,
    assertion_text: String,
    local: String,
    source_hashes: Vec<String>,
    spans: Vec<(u32, u32)>,
    dependency_semantic: String,
    dependency_validation: String,
    pub(in crate::analysis) semantic_digest: String,
    validation_digest: String,
}

fn hash_fields(fields: &[String]) -> String {
    let mut hash = Sha256::new();
    for field in fields {
        hash.update((field.len() as u64).to_le_bytes());
        hash.update(field.as_bytes());
    }
    format!("{:x}", hash.finalize())
}

fn source_hash(source: &str) -> String {
    format!("{:x}", Sha256::digest(source.as_bytes()))
}

impl FallibleCheckReceipt {
    fn semantic_fields(&self) -> Vec<String> {
        vec![
            "fallible-owner-literal-7a-v1".into(),
            self.owner.0.clone(),
            self.helper.0.clone(),
            self.test.0.clone(),
            self.test_name.clone(),
            self.owner_value.to_string(),
            self.expected_value.to_string(),
            self.source_identity.clone(),
            self.local.clone(),
            self.dependency_semantic.clone(),
        ]
    }

    fn validation_fields(&self) -> Vec<String> {
        let mut fields = self.semantic_fields();
        fields.extend(self.source_hashes.clone());
        fields.extend(
            self.spans
                .iter()
                .map(|(start, end)| format!("{start}:{end}")),
        );
        fields.extend([
            self.source_file.to_string_lossy().into_owned(),
            self.source_line.to_string(),
            self.test_file.to_string_lossy().into_owned(),
            self.test_line.to_string(),
            self.assertion_line.to_string(),
            self.assertion_text.clone(),
            self.dependency_validation.clone(),
        ]);
        fields
    }

    pub(in crate::analysis) fn valid_for(&self, probe: &Probe) -> bool {
        probe.family == ProbeFamily::ReturnValue
            && probe.owner.as_ref() == Some(&self.owner)
            && probe.expression.trim() == self.source_identity
            && probe.location.file == self.source_file
            && probe.location.line == self.source_line
            && self.owner_value == self.expected_value
            && decimal_u8(&self.source_identity) == Some(self.owner_value)
            && self.source_hashes.len() == 2
            && !self.spans.is_empty()
            && self.semantic_digest == hash_fields(&self.semantic_fields())
            && self.validation_digest == hash_fields(&self.validation_fields())
    }

    pub(in crate::analysis) fn admits(&self, test: &TestSummary, assertion: &OracleFact) -> bool {
        self.test_file == test.file
            && self.test_name == test.name
            && self.test_line == test.start_line
            && self.assertion_line == assertion.line
            && self.assertion_text == assertion.text.trim()
    }
}

impl OwnerPinSyntax {
    fn fallible_dependency(&self, root: &Path) -> Option<AnyhowIdentity> {
        let snapshot = config_snapshot_identity(root)?;
        let mut memo = self.fallible_dependencies.borrow_mut();
        if let Some((previous, identity)) = memo.get(root)
            && *previous == snapshot
        {
            return identity.clone();
        }
        let identity = verified_anyhow_identity(root);
        memo.insert(root.to_path_buf(), (snapshot, identity.clone()));
        identity
    }
}

pub(in crate::analysis) fn fallible_check_receipt(
    probe: &Probe,
    owner: &FunctionSummary,
    test: &TestSummary,
    index: &RustIndex,
    syntax: &OwnerPinSyntax,
) -> Option<FallibleCheckReceipt> {
    if probe.family != ProbeFamily::ReturnValue
        || probe.owner.as_ref() != Some(&owner.id)
        || owner.source_role != FunctionSourceRole::Production
        || !matches!(owner.item.container, FunctionContainer::Free)
        || !test
            .assertions
            .iter()
            .any(|a| a.text.trim().starts_with("ensure!("))
    {
        return None;
    }
    let parent = index.files().get(&owner.file)?;
    let child = index.files().get(&test.file)?;
    let analysis_root = index
        .workspace_authority
        .as_ref()?
        .root
        .canonicalize()
        .ok()?;
    let owner_path = analysis_root.join(&owner.file);
    let test_path = analysis_root.join(&test.file);
    if parent.used_lexical_fallback
        || child.used_lexical_fallback
        || child.role_provenance.earliest_unresolved_reason.is_some()
    {
        return None;
    }
    let owner_syntax = literal_owner_syntax(&parent.source, owner)?;
    let [edge] = child.role_provenance.edges.as_slice() else {
        return None;
    };
    if edge.kind != SourceRoleProvenanceEdgeKind::Module
        || !edge.requires_test
        || edge.parent != owner.file
        || edge.child != test.file
        || edge.line != owner_syntax.module_line
        || edge.declaration != format!("mod {};", owner_syntax.module_name)
    {
        return None;
    }
    let [declaration] = parent.module_declarations.as_slice() else {
        return None;
    };
    let ModulePathTarget::Literal(path) = &declaration.path_target else {
        return None;
    };
    if declaration.name != owner_syntax.module_name
        || declaration.line != edge.line
        || !declaration.requires_test
        || owner_path.parent()?.join(path).canonicalize().ok()? != test_path.canonicalize().ok()?
    {
        return None;
    }
    let literal_start = u32::from(owner_syntax.literal.start()) as usize;
    let literal_end = u32::from(owner_syntax.literal.end()) as usize;
    let source_identity = parent.source.get(literal_start..literal_end)?;
    let source_line = 1 + parent
        .source
        .get(..literal_start)?
        .bytes()
        .filter(|b| *b == b'\n')
        .count();
    if probe.expression.trim() != source_identity
        || probe.location.line != source_line
        || probe
            .location
            .file
            .canonicalize()
            .or_else(|_| analysis_root.join(&probe.location.file).canonicalize())
            .ok()?
            != owner_path.canonicalize().ok()?
    {
        return None;
    }
    let check = literal_check_syntax(&child.source, test, &owner.name)?;
    // Refuse inverted oracles before constructing any favorable evidence.
    if owner_syntax.value != check.expected {
        return None;
    }
    let helpers = index
        .functions()
        .iter()
        .filter(|f| {
            f.file == test.file && f.name == check.helper_name && f.start_line == check.helper_line
        })
        .collect::<Vec<_>>();
    let tests = index
        .functions()
        .iter()
        .filter(|f| f.file == test.file && f.name == test.name && f.start_line == test.start_line)
        .collect::<Vec<_>>();
    let ([helper], [test_function]) = (helpers.as_slice(), tests.as_slice()) else {
        return None;
    };
    if helper.source_role != FunctionSourceRole::CfgTestModule
        || index
            .functions()
            .iter()
            .filter(|f| f.name == owner.name)
            .count()
            != 1
    {
        return None;
    }
    let oracle = test
        .assertions
        .iter()
        .filter(|a| {
            a.line == check.assertion_line
                && a.text.trim() == check.assertion_text
                && a.kind == crate::domain::OracleKind::ExactValue
                && a.strength == crate::domain::OracleStrength::Strong
        })
        .collect::<Vec<_>>();
    if oracle.len() != 1 {
        return None;
    }
    let root = owner_path
        .parent()?
        .ancestors()
        .find(|p| p.join("Cargo.toml").is_file())?;
    // The initial cache/dependency scope is one standalone owning package.
    if analysis_root != root.canonicalize().ok()?
        || !test_path
            .canonicalize()
            .ok()?
            .starts_with(root.canonicalize().ok()?)
    {
        return None;
    }
    // Source re-reads bind the immutable parser index to the dependency probe.
    if std::fs::read_to_string(&owner_path).ok()?.as_str() != parent.source.as_ref()
        || std::fs::read_to_string(&test_path).ok()?.as_str() != child.source.as_ref()
    {
        return None;
    }
    let dependency = syntax.fallible_dependency(root)?;
    if owner_path.canonicalize().ok()? != dependency.library_root
        || std::fs::read_to_string(&owner_path).ok()?.as_str() != parent.source.as_ref()
        || std::fs::read_to_string(&test_path).ok()?.as_str() != child.source.as_ref()
    {
        return None;
    }
    let mut spans = vec![
        (literal_start as u32, literal_end as u32),
        (edge.line as u32, edge.line as u32),
    ];
    spans.extend(check.spans);
    let mut receipt = FallibleCheckReceipt {
        owner: owner.id.clone(),
        helper: helper.id.clone(),
        test: test_function.id.clone(),
        owner_value: owner_syntax.value,
        expected_value: check.expected,
        source_identity: source_identity.into(),
        source_file: probe.location.file.clone(),
        source_line,
        test_file: test.file.clone(),
        test_name: test.name.clone(),
        test_line: test.start_line,
        assertion_line: check.assertion_line,
        assertion_text: check.assertion_text,
        local: check.local,
        source_hashes: vec![source_hash(&parent.source), source_hash(&child.source)],
        spans,
        dependency_semantic: dependency.semantic_digest,
        dependency_validation: dependency.validation_digest,
        semantic_digest: String::new(),
        validation_digest: String::new(),
    };
    receipt.semantic_digest = hash_fields(&receipt.semantic_fields());
    receipt.validation_digest = hash_fields(&receipt.validation_fields());
    receipt.valid_for(probe).then_some(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::classify::propagation_witness::{
        complete_direct_witness, fallible_literal_witness,
    };
    use crate::domain::{DeltaKind, ProbeId, SourceLocation};
    use crate::domain::{FlowSinkFact, FlowSinkKind};

    #[test]
    fn literal_witness_rejects_receipt_and_owner_identity_mutations() -> Result<(), String> {
        let probe = Probe {
            id: ProbeId("probe:literal".into()),
            location: SourceLocation::new("src/lib.rs", 1, 1),
            owner: Some(SymbolId("owner:answer".into())),
            family: ProbeFamily::ReturnValue,
            delta: DeltaKind::Value,
            before: Some("7".into()),
            after: Some("7".into()),
            expression: "7".into(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        };
        let mut receipt = FallibleCheckReceipt {
            owner: probe.owner.clone().ok_or("owner")?,
            helper: SymbolId("helper:check_answer".into()),
            test: SymbolId("test:observes_answer".into()),
            owner_value: 7,
            expected_value: 7,
            source_identity: "7".into(),
            source_file: probe.location.file.clone(),
            source_line: 1,
            test_file: PathBuf::from("src/test_support.rs"),
            test_name: "observes_answer".into(),
            test_line: 11,
            assertion_line: 5,
            assertion_text: "ensure!(actual == 7);".into(),
            local: "actual".into(),
            source_hashes: vec!["owner-snapshot".into(), "helper-snapshot".into()],
            spans: vec![(0, 1)],
            dependency_semantic: "canonical-anyhow".into(),
            dependency_validation: "locked-config-snapshot".into(),
            semantic_digest: String::new(),
            validation_digest: String::new(),
        };
        receipt.semantic_digest = hash_fields(&receipt.semantic_fields());
        receipt.validation_digest = hash_fields(&receipt.validation_fields());
        let sink = FlowSinkFact {
            kind: FlowSinkKind::ReturnValue,
            text: "7".into(),
            line: 1,
            owner: probe.owner.clone(),
        };
        let witness = fallible_literal_witness(&probe, &[sink], &receipt)
            .ok_or("complete literal witness")?;
        assert!(complete_direct_witness(&probe, Some(&witness)));
        for field in [
            "expected",
            "source",
            "dependency",
            "coordinate",
            "span",
            "helper",
        ] {
            let mut changed = receipt.clone();
            match field {
                "expected" => changed.expected_value = 8,
                "source" => changed.source_hashes[1] = "changed".into(),
                "dependency" => changed.dependency_validation = "changed".into(),
                "coordinate" => changed.source_line = 2,
                "span" => changed.spans[0] = (1, 2),
                "helper" => changed.helper = SymbolId("other-helper".into()),
                _ => return Err("unknown mutation control".into()),
            }
            assert!(!changed.valid_for(&probe), "{field}");
            let mut forged = witness.clone();
            forged.fallible_check = Some(changed);
            forged.semantic_digest = forged.compute_semantic_digest();
            assert!(
                !complete_direct_witness(&probe, Some(&forged)),
                "{field}: outer digest alone is insufficient"
            );
        }
        let mut other = probe.clone();
        other.location.file = PathBuf::from("other/lib.rs");
        assert!(!complete_direct_witness(&other, Some(&witness)));
        other = probe;
        other.owner = Some(SymbolId("other-owner".into()));
        assert!(!complete_direct_witness(&other, Some(&witness)));
        Ok(())
    }

    // ---- receipt validity, admission and digest controls ----

    fn base_probe() -> Probe {
        Probe {
            id: ProbeId("probe:literal".into()),
            location: SourceLocation::new("src/lib.rs", 1, 1),
            owner: Some(SymbolId("owner:answer".into())),
            family: ProbeFamily::ReturnValue,
            delta: DeltaKind::Value,
            before: Some("7".into()),
            after: Some("7".into()),
            expression: "7".into(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        }
    }

    fn base_receipt(probe: &Probe) -> Result<FallibleCheckReceipt, String> {
        let mut receipt = FallibleCheckReceipt {
            owner: probe.owner.clone().ok_or("owner")?,
            helper: SymbolId("helper:check_answer".into()),
            test: SymbolId("test:observes_answer".into()),
            owner_value: 7,
            expected_value: 7,
            source_identity: "7".into(),
            source_file: probe.location.file.clone(),
            source_line: 1,
            test_file: PathBuf::from("src/test_support.rs"),
            test_name: "observes_answer".into(),
            test_line: 11,
            assertion_line: 5,
            assertion_text: "ensure!(actual == 7);".into(),
            local: "actual".into(),
            source_hashes: vec!["owner-snapshot".into(), "helper-snapshot".into()],
            spans: vec![(0, 1)],
            dependency_semantic: "canonical-anyhow".into(),
            dependency_validation: "locked-config-snapshot".into(),
            semantic_digest: String::new(),
            validation_digest: String::new(),
        };
        reseal(&mut receipt);
        Ok(receipt)
    }

    type Defect<T> = (&'static str, Box<dyn Fn(&mut T)>);

    fn reseal(receipt: &mut FallibleCheckReceipt) {
        receipt.semantic_digest = hash_fields(&receipt.semantic_fields());
        receipt.validation_digest = hash_fields(&receipt.validation_fields());
    }

    #[test]
    fn hash_fields_length_prefixes_so_field_boundaries_are_binding() {
        let strings = |items: &[&str]| items.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        let joined = hash_fields(&strings(&["ab", "c"]));
        assert_eq!(joined, hash_fields(&strings(&["ab", "c"])), "deterministic");
        assert_ne!(
            joined,
            hash_fields(&strings(&["a", "bc"])),
            "boundary shift"
        );
        assert_ne!(joined, hash_fields(&strings(&["abc"])), "concatenation");
        assert_ne!(
            hash_fields(&strings(&["", "x"])),
            hash_fields(&strings(&["x", ""])),
            "empty field position"
        );
        assert_eq!(joined.len(), 64);
        assert!(joined.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn source_hash_is_the_plain_sha256_hex_of_the_bytes() {
        assert_eq!(
            source_hash(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            source_hash("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_ne!(source_hash("abc"), source_hash("abc\n"));
    }

    #[test]
    fn semantic_digest_covers_identity_but_not_coordinates_and_validation_covers_both()
    -> Result<(), String> {
        let probe = base_probe();
        let base = base_receipt(&probe)?;
        assert!(base.valid_for(&probe), "positive control");
        let moved = |mutate: &dyn Fn(&mut FallibleCheckReceipt)| {
            let mut changed = base.clone();
            mutate(&mut changed);
            reseal(&mut changed);
            (
                changed.semantic_digest != base.semantic_digest,
                changed.validation_digest != base.validation_digest,
            )
        };
        // Semantic identity moves both digests.
        assert_eq!(moved(&|r| r.helper = SymbolId("h2".into())), (true, true));
        assert_eq!(moved(&|r| r.test = SymbolId("t2".into())), (true, true));
        assert_eq!(moved(&|r| r.test_name = "other".into()), (true, true));
        assert_eq!(moved(&|r| r.local = "other".into()), (true, true));
        assert_eq!(moved(&|r| r.dependency_semantic = "x".into()), (true, true));
        assert_eq!(moved(&|r| r.owner_value = 8), (true, true));
        // Coordinates and hashes move only the validation digest.
        assert_eq!(moved(&|r| r.source_line = 2), (false, true));
        assert_eq!(moved(&|r| r.test_line = 12), (false, true));
        assert_eq!(moved(&|r| r.assertion_line = 6), (false, true));
        assert_eq!(
            moved(&|r| r.assertion_text = "ensure!(actual == 7u8);".into()),
            (false, true)
        );
        assert_eq!(
            moved(&|r| r.test_file = PathBuf::from("src/other.rs")),
            (false, true)
        );
        assert_eq!(
            moved(&|r| r.source_file = PathBuf::from("src/other.rs")),
            (false, true)
        );
        assert_eq!(moved(&|r| r.spans.push((3, 4))), (false, true));
        assert_eq!(
            moved(&|r| r.source_hashes[0] = "changed".into()),
            (false, true)
        );
        assert_eq!(
            moved(&|r| r.dependency_validation = "x".into()),
            (false, true)
        );
        Ok(())
    }

    #[test]
    fn valid_for_refuses_each_independent_defect() -> Result<(), String> {
        let probe = base_probe();
        let base = base_receipt(&probe)?;
        assert!(base.valid_for(&probe), "positive control");

        // Probe-side defects against an untouched receipt.
        let probe_defects: Vec<Defect<Probe>> = vec![
            ("family", Box::new(|p| p.family = ProbeFamily::Predicate)),
            ("owner", Box::new(|p| p.owner = Some(SymbolId("x".into())))),
            ("no_owner", Box::new(|p| p.owner = None)),
            ("expression", Box::new(|p| p.expression = "8".into())),
            (
                "file",
                Box::new(|p| p.location.file = PathBuf::from("a.rs")),
            ),
            ("line", Box::new(|p| p.location.line = 2)),
        ];
        for (name, mutate) in probe_defects {
            let mut changed = probe.clone();
            mutate(&mut changed);
            assert!(!base.valid_for(&changed), "{name}");
        }
        // Whitespace around the probe expression is tolerated.
        let mut padded = probe.clone();
        padded.expression = "  7\n".into();
        assert!(base.valid_for(&padded), "trimmed expression");

        // Receipt-side defects, each resealed so only the named gate can fire.
        let sealed_defects: Vec<Defect<FallibleCheckReceipt>> = vec![
            ("inverted", Box::new(|r| r.expected_value = 8)),
            (
                "non_decimal_identity",
                Box::new(|r| r.source_identity = "0x7".into()),
            ),
            (
                "identity_value_mismatch",
                Box::new(|r| {
                    r.owner_value = 8;
                    r.expected_value = 8;
                }),
            ),
            (
                "one_hash",
                Box::new(|r| {
                    r.source_hashes.pop();
                }),
            ),
            (
                "three_hashes",
                Box::new(|r| r.source_hashes.push("extra".into())),
            ),
            ("no_spans", Box::new(|r| r.spans.clear())),
        ];
        for (name, mutate) in sealed_defects {
            let mut changed = base.clone();
            mutate(&mut changed);
            reseal(&mut changed);
            assert!(!changed.valid_for(&probe), "{name}");
        }
        // Source spellings: `decimal_u8` admits a leading zero and a `u8`
        // suffix (observed behavior, pinned), but refuses a sign or radix.
        for (spelling, accepted) in [
            ("07", true),
            ("7u8", true),
            ("0_7", true),
            ("-7", false),
            ("+7", false),
            ("0x7", false),
            ("7.0", false),
        ] {
            let mut spelled = base.clone();
            spelled.source_identity = spelling.into();
            reseal(&mut spelled);
            let mut spelled_probe = probe.clone();
            spelled_probe.expression = spelling.into();
            assert_eq!(spelled.valid_for(&spelled_probe), accepted, "{spelling}");
        }
        // A reseal of an unmodified receipt stays valid, so the resealed
        // refusals above are attributable to the mutation alone.
        let mut resealed = base.clone();
        reseal(&mut resealed);
        assert!(resealed.valid_for(&probe));

        // Stale digests without a reseal.
        let mut stale_semantic = base.clone();
        stale_semantic.semantic_digest = "0".repeat(64);
        assert!(!stale_semantic.valid_for(&probe), "semantic digest");
        let mut stale_validation = base.clone();
        stale_validation.validation_digest = String::new();
        assert!(!stale_validation.valid_for(&probe), "validation digest");
        let mut unsealed_move = base.clone();
        unsealed_move.assertion_line = 99;
        assert!(!unsealed_move.valid_for(&probe), "unsealed coordinate move");
        Ok(())
    }

    #[test]
    fn admits_binds_the_exact_test_and_assertion_coordinates() -> Result<(), String> {
        let probe = base_probe();
        let receipt = base_receipt(&probe)?;
        let facts = crate::analysis::rust_index::summarize_file(
            "src/test_support.rs".into(),
            "use anyhow::{Result, ensure};\n\nfn check_answer() -> Result<()> {\n    let actual = super::answer();\n    ensure!(actual == 7);\n    Ok(())\n}\n\n#[test]\nfn observes_answer() -> Result<()> {\n    check_answer()\n}\n"
                .to_string(),
        );
        let mut test = facts
            .tests
            .into_iter()
            .find(|t| t.name == "observes_answer")
            .ok_or("premise: observes_answer parses")?;
        test.start_line = receipt.test_line;
        let assertion = OracleFact {
            line: receipt.assertion_line,
            text: "  ensure!(actual == 7);\n".into(),
            kind: crate::domain::OracleKind::ExactValue,
            strength: crate::domain::OracleStrength::Strong,
            observed_tokens: Vec::new(),
            ok_value_observed: None,
        };
        assert_eq!(test.file, receipt.test_file, "premise: same file");
        assert!(receipt.admits(&test, &assertion), "positive control");

        let mut other_file = test.clone();
        other_file.file = PathBuf::from("src/other.rs");
        assert!(!receipt.admits(&other_file, &assertion), "file");
        let mut other_name = test.clone();
        other_name.name = "other".into();
        assert!(!receipt.admits(&other_name, &assertion), "name");
        let mut other_line = test.clone();
        other_line.start_line += 1;
        assert!(!receipt.admits(&other_line, &assertion), "test line");
        let mut moved = assertion.clone();
        moved.line += 1;
        assert!(!receipt.admits(&test, &moved), "assertion line");
        let mut other_text = assertion.clone();
        other_text.text = "ensure!(actual == 8);".into();
        assert!(!receipt.admits(&test, &other_text), "assertion text");
        Ok(())
    }

    // ---- fallible_check_receipt over a real on-disk package ----

    use crate::analysis::facts::{RustIndex, build_index};
    use crate::analysis::workspace::{AnyhowIdentity, config_snapshot_identity};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    const MANIFEST: &str = "[package]\nname = \"classify-fallible-check\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n[dependencies]\nanyhow = \"=1.0.100\"\n\n[workspace]\n";
    const OWNER_SRC: &str =
        "pub fn answer() -> u8 { 7 }\n\n#[cfg(test)]\n#[path = \"test_support.rs\"]\nmod tests;\n";
    const CHECK_SRC: &str = "use anyhow::{Result, ensure};\n\nfn check_answer() -> Result<()> {\n    let actual = super::answer();\n    ensure!(actual == 7);\n    Ok(())\n}\n\n#[test]\nfn observes_answer() -> Result<()> {\n    check_answer()\n}\n";

    struct TempRoot(PathBuf);

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    struct Package {
        root: TempRoot,
        index: RustIndex,
        probe: Probe,
        owner: FunctionSummary,
        test: TestSummary,
    }

    impl Package {
        fn lib(&self) -> PathBuf {
            self.root.0.join("src/lib.rs")
        }

        fn syntax(&self, library_root: Option<PathBuf>) -> Result<OwnerPinSyntax, String> {
            let canonical = self.root.0.canonicalize().map_err(|e| e.to_string())?;
            let snapshot = config_snapshot_identity(&canonical)
                .ok_or("premise: config snapshot is available")?;
            let syntax = OwnerPinSyntax::default();
            let library_root = match library_root {
                Some(path) => path,
                None => self.lib().canonicalize().map_err(|e| e.to_string())?,
            };
            syntax.fallible_dependencies.borrow_mut().insert(
                canonical,
                (
                    snapshot,
                    Some(AnyhowIdentity {
                        semantic_digest: "canonical-anyhow".into(),
                        validation_digest: "locked-config-snapshot".into(),
                        library_root,
                    }),
                ),
            );
            Ok(syntax)
        }

        fn receipt(&self) -> Result<Option<FallibleCheckReceipt>, String> {
            let syntax = self.syntax(None)?;
            Ok(fallible_check_receipt(
                &self.probe,
                &self.owner,
                &self.test,
                &self.index,
                &syntax,
            ))
        }
    }

    fn package(owner_src: &str, check_src: &str) -> Result<Package, String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-classify-fallible-{}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(dir.join("src")).map_err(|e| e.to_string())?;
        let root = TempRoot(dir);
        std::fs::write(root.0.join("Cargo.toml"), MANIFEST).map_err(|e| e.to_string())?;
        std::fs::write(root.0.join("src/lib.rs"), owner_src).map_err(|e| e.to_string())?;
        std::fs::write(root.0.join("src/test_support.rs"), check_src).map_err(|e| e.to_string())?;
        let index = build_index(
            &root.0,
            &[
                PathBuf::from("src/lib.rs"),
                PathBuf::from("src/test_support.rs"),
            ],
        )?;
        let owner = index
            .functions()
            .iter()
            .find(|f| f.name == "answer" && f.file == Path::new("src/lib.rs"))
            .cloned()
            .ok_or("premise: owner `answer` is indexed")?;
        let test = index
            .tests()
            .iter()
            .find(|t| t.name == "observes_answer")
            .cloned()
            .ok_or("premise: test `observes_answer` is indexed")?;
        let mut probe = base_probe();
        probe.owner = Some(owner.id.clone());
        // Absolute, so the probe path never resolves against the test
        // process's working directory (the ripr crate has its own src/lib.rs).
        probe.location.file = root.0.join("src/lib.rs");
        Ok(Package {
            root,
            index,
            probe,
            owner,
            test,
        })
    }

    #[test]
    fn receipt_is_issued_for_the_closed_shape_and_pins_every_field() -> Result<(), String> {
        let pkg = package(OWNER_SRC, CHECK_SRC)?;
        let receipt = pkg
            .receipt()?
            .ok_or("positive control: closed shape issues a receipt")?;
        assert_eq!(receipt.owner, pkg.owner.id);
        assert_eq!(receipt.test_name, "observes_answer");
        assert_eq!(receipt.test_file, PathBuf::from("src/test_support.rs"));
        assert_eq!(receipt.test_line, pkg.test.start_line);
        assert_eq!(receipt.owner_value, 7);
        assert_eq!(receipt.expected_value, 7);
        assert_eq!(receipt.source_identity, "7");
        assert_eq!(receipt.source_line, 1);
        assert_eq!(receipt.source_file, pkg.lib());
        assert_eq!(receipt.assertion_line, 5);
        assert_eq!(receipt.assertion_text, "ensure!(actual == 7);");
        assert_eq!(receipt.local, "actual");
        assert_eq!(
            receipt.source_hashes,
            vec![source_hash(OWNER_SRC), source_hash(CHECK_SRC)]
        );
        // The literal span comes first, then the module declaration line,
        // then the check spans.
        assert_eq!(receipt.spans[0], (24, 25));
        assert!(receipt.spans.len() > 2);
        assert_eq!(receipt.dependency_semantic, "canonical-anyhow");
        assert_eq!(receipt.dependency_validation, "locked-config-snapshot");
        assert!(receipt.valid_for(&pkg.probe));
        assert_eq!(
            receipt.semantic_digest,
            hash_fields(&receipt.semantic_fields())
        );
        // The receipt admits exactly the assertion it names.
        let assertion = pkg
            .test
            .assertions
            .iter()
            .find(|a| a.text.trim() == "ensure!(actual == 7);")
            .ok_or("premise: the ensure! assertion is credited to the test")?;
        assert!(receipt.admits(&pkg.test, assertion));
        Ok(())
    }

    #[test]
    fn receipt_refuses_probe_owner_and_test_defects_after_a_positive_control() -> Result<(), String>
    {
        let pkg = package(OWNER_SRC, CHECK_SRC)?;
        assert!(pkg.receipt()?.is_some(), "positive control");
        let syntax = pkg.syntax(None)?;
        let run = |probe: &Probe, owner: &FunctionSummary, test: &TestSummary| {
            fallible_check_receipt(probe, owner, test, &pkg.index, &syntax)
        };

        let mut wrong_family = pkg.probe.clone();
        wrong_family.family = ProbeFamily::Predicate;
        assert!(
            run(&wrong_family, &pkg.owner, &pkg.test).is_none(),
            "family"
        );

        let mut other_owner = pkg.probe.clone();
        other_owner.owner = Some(SymbolId("someone-else".into()));
        assert!(
            run(&other_owner, &pkg.owner, &pkg.test).is_none(),
            "owner id"
        );

        let mut no_owner = pkg.probe.clone();
        no_owner.owner = None;
        assert!(run(&no_owner, &pkg.owner, &pkg.test).is_none(), "no owner");

        let mut wrong_expression = pkg.probe.clone();
        wrong_expression.expression = "8".into();
        assert!(
            run(&wrong_expression, &pkg.owner, &pkg.test).is_none(),
            "expression"
        );

        let mut wrong_line = pkg.probe.clone();
        wrong_line.location.line = 2;
        assert!(run(&wrong_line, &pkg.owner, &pkg.test).is_none(), "line");

        let mut wrong_file = pkg.probe.clone();
        wrong_file.location.file = pkg.root.0.join("src/test_support.rs");
        assert!(run(&wrong_file, &pkg.owner, &pkg.test).is_none(), "file");

        let mut test_role = pkg.owner.clone();
        test_role.source_role = FunctionSourceRole::TestAttribute;
        assert!(run(&pkg.probe, &test_role, &pkg.test).is_none(), "role");

        for container in [
            FunctionContainer::Unknown,
            FunctionContainer::Local,
            FunctionContainer::Inherent {
                self_ty: "S".into(),
            },
        ] {
            let mut owner = pkg.owner.clone();
            owner.item.container = container.clone();
            assert!(
                run(&pkg.probe, &owner, &pkg.test).is_none(),
                "container {container:?}"
            );
        }

        let mut no_ensure = pkg.test.clone();
        no_ensure.assertions.clear();
        assert!(run(&pkg.probe, &pkg.owner, &no_ensure).is_none(), "ensure!");
        let mut other_assertion = pkg.test.clone();
        for a in &mut other_assertion.assertions {
            a.text = "assert_eq!(actual, 7);".into();
        }
        assert!(
            run(&pkg.probe, &pkg.owner, &other_assertion).is_none(),
            "non-ensure assertion"
        );

        let mut unindexed = pkg.test.clone();
        unindexed.file = PathBuf::from("src/missing.rs");
        assert!(run(&pkg.probe, &pkg.owner, &unindexed).is_none(), "file");
        let mut moved_test = pkg.test.clone();
        moved_test.start_line += 1;
        assert!(run(&pkg.probe, &pkg.owner, &moved_test).is_none(), "line");

        // Missing workspace authority refuses even the positive shape.
        let mut no_authority = pkg.index.clone();
        no_authority.workspace_authority = None;
        assert!(
            fallible_check_receipt(&pkg.probe, &pkg.owner, &pkg.test, &no_authority, &syntax)
                .is_none(),
            "workspace authority"
        );
        Ok(())
    }

    #[test]
    fn receipt_refuses_inverted_and_off_grammar_sources() -> Result<(), String> {
        let control = package(OWNER_SRC, CHECK_SRC)?;
        assert!(control.receipt()?.is_some(), "positive control");
        for (name, owner_src, check_src) in [
            (
                "inverted",
                OWNER_SRC.to_string(),
                CHECK_SRC.replace("== 7", "== 8"),
            ),
            (
                "owner_value_changed",
                OWNER_SRC.replace("{ 7 }", "{ 8 }"),
                CHECK_SRC.to_string(),
            ),
            (
                "extra_owner_item",
                format!("{OWNER_SRC}fn other() {{}}\n"),
                CHECK_SRC.to_string(),
            ),
            (
                "not_ensure",
                OWNER_SRC.to_string(),
                CHECK_SRC.replace("ensure!(actual == 7);", "assert_eq!(actual, 7);"),
            ),
            (
                "trailing_statement",
                OWNER_SRC.to_string(),
                CHECK_SRC.replace(
                    "ensure!(actual == 7);",
                    "ensure!(actual == 7);\n    let _ = 1;",
                ),
            ),
            (
                "second_owner_definition",
                OWNER_SRC.to_string(),
                format!("{CHECK_SRC}\nfn answer() -> u8 {{ 7 }}\n"),
            ),
        ] {
            let pkg = package(&owner_src, &check_src)?;
            assert_eq!(pkg.owner.name, "answer", "{name}: premise");
            assert_eq!(pkg.test.name, "observes_answer", "{name}: premise");
            assert!(pkg.receipt()?.is_none(), "{name}");
        }
        Ok(())
    }

    #[test]
    fn receipt_refuses_dependency_and_source_drift() -> Result<(), String> {
        let pkg = package(OWNER_SRC, CHECK_SRC)?;
        let before = pkg.receipt()?;
        assert!(before.is_some(), "positive control");

        // Verified dependency whose library root is not the owner file.
        let other_root = pkg
            .root
            .0
            .join("src/test_support.rs")
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let syntax = pkg.syntax(Some(other_root))?;
        assert!(
            fallible_check_receipt(&pkg.probe, &pkg.owner, &pkg.test, &pkg.index, &syntax)
                .is_none(),
            "library root mismatch"
        );

        // A memoized refusal (dependency unverifiable) is final.
        let canonical = pkg.root.0.canonicalize().map_err(|e| e.to_string())?;
        let snapshot = config_snapshot_identity(&canonical).ok_or("premise: config snapshot")?;
        let refused = OwnerPinSyntax::default();
        refused
            .fallible_dependencies
            .borrow_mut()
            .insert(canonical, (snapshot, None));
        assert!(
            fallible_check_receipt(&pkg.probe, &pkg.owner, &pkg.test, &pkg.index, &refused)
                .is_none(),
            "unverified dependency"
        );

        // On-disk drift after indexing: the parsed index no longer binds.
        let syntax = pkg.syntax(None)?;
        std::fs::write(pkg.lib(), format!("{OWNER_SRC}// drift\n")).map_err(|e| e.to_string())?;
        assert!(
            fallible_check_receipt(&pkg.probe, &pkg.owner, &pkg.test, &pkg.index, &syntax)
                .is_none(),
            "owner drift"
        );
        std::fs::write(pkg.lib(), OWNER_SRC).map_err(|e| e.to_string())?;
        assert!(
            fallible_check_receipt(&pkg.probe, &pkg.owner, &pkg.test, &pkg.index, &syntax)
                == before,
            "restored source reproduces the pre-drift receipt"
        );
        std::fs::write(
            pkg.root.0.join("src/test_support.rs"),
            format!("{CHECK_SRC}// drift\n"),
        )
        .map_err(|e| e.to_string())?;
        assert!(
            fallible_check_receipt(&pkg.probe, &pkg.owner, &pkg.test, &pkg.index, &syntax)
                .is_none(),
            "test drift"
        );
        Ok(())
    }
}
