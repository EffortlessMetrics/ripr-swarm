//! Exact canonical binding of passive retained observations. This owner cannot
//! create a live process observation or promote the canonical scoreboard.
use super::*;
use crate::formal_evidence as formal;

pub const USAGE: &str = "formal-claim-registry replay-native --root DIR --bindings FILE --run FILE --artifacts DIR\nExit 0 means retained consistency only; native origin and assurance remain unestablished.";

impl PreparedNativeInputs {
    pub fn replay_native(&self, run: &formal::Run, artifacts: &Path) -> RegistryResult<String> {
        self.registry.require_registry_bytes()?;
        require(
            run.repository == self.registry.loaded.manifest.repository,
            "retained run repository differs from canonical registry",
        )?;
        require(
            run.claim_registry_generation == self.registry.generation(),
            "retained run registry generation mismatch",
        )?;
        let execution = run.execution.as_ref().ok_or("retained execution absent")?;
        require(
            Path::new(&execution.repository_directory) == self.registry.root(),
            "retained source root differs from local preparation",
        )?;
        let profile = native::DeclaredKaniProfile::for_policy(&execution.configuration_policy)?;
        let replay = native::verify_execution_artifacts(run, artifacts, &profile)?;
        let retained: serde_json::Value = formal::parse_json(
            replay
                .registration()
                .ok_or("retained canonical preparation absent")?,
        )?;
        let local: serde_json::Value = formal::parse_json(self.to_json()?.as_bytes())?;
        require(
            retained == local,
            "retained preparation differs from locally validated canonical inputs",
        )?;
        let mut rows = Vec::new();
        let mut mapped = BTreeSet::new();
        for prepared in &self.claims {
            let mut missing = prepared.missing_obligations.clone();
            let mut observations = Vec::new();
            if let Some(binding) = &prepared.declaration {
                for subject in &prepared.subjects {
                    let observation = replay
                        .observations()
                        .iter()
                        .find(|row| row.subject == subject.native);
                    let Some(observation) = observation else {
                        missing.push(format!("native observation: {}", subject.canonical_subject));
                        continue;
                    };
                    require(
                        binding.profile == profile,
                        "canonical binding and retained native profile differ",
                    )?;
                    require(
                        prepared.source_manifest == *replay.sources(),
                        "retained source closure is omitted, substituted or differs from local preparation",
                    )?;
                    require(
                        execution.package_inputs.get(&subject.native.package)
                            == Some(&prepared.explicit_inputs),
                        "retained explicit inputs differ from local preparation",
                    )?;
                    mapped.insert(observation.subject.id());
                    if prepared.binding_state != "prepared" {
                        continue;
                    }
                    let expected: Vec<_> = binding
                        .covers
                        .iter()
                        .filter(|cover| cover.canonical_subject == subject.canonical_subject)
                        .collect();
                    require(
                        expected.len() == observation.cover_properties.len(),
                        "native cover inventory differs from exact canonical obligations",
                    )?;
                    let mut covers = Vec::new();
                    for cover in expected {
                        let presentation = cover
                            .presentation
                            .as_ref()
                            .ok_or("canonical cover presentation absent")?;
                        let property = observation
                            .cover_properties
                            .iter()
                            .find(|property| property.name == presentation.property)
                            .ok_or("canonical native cover property is missing")?;
                        require(
                            property.description.as_ref() == Some(&presentation.description)
                                && property.location.as_ref() == Some(&presentation.location()),
                            "canonical native cover presentation differs from retained property",
                        )?;
                        covers.push(serde_json::json!({"obligation":cover.obligation,"property":property.name,"native_status":property.status}));
                    }
                    if !execution.source_current || !execution.configuration_admitted {
                        missing
                            .push("complete retained source/configuration stability".to_string());
                    }
                    observations.push(serde_json::json!({
                        "canonical_subject":subject.canonical_subject,"native_subject":observation.subject,
                        "native_status":observation.status,"native_verdict":observation.native_verdict,
                        "covers":covers,"raw_block_digest":observation.raw_block_digest,
                        "interpretation":"reproduced_from_supplied_bytes"
                    }));
                }
            }
            missing.extend(
                prepared
                    .negative_controls
                    .iter()
                    .map(|control| format!("native negative-control pair: {control}")),
            );
            missing.extend([
                "authenticated native origin".to_string(),
                "actual compiler input/configuration consumption".to_string(),
                "protected lifecycle/currentness and transition admission (#14500, #6452/#6456)"
                    .to_string(),
            ]);
            rows.push(serde_json::json!({
                "claim_id":prepared.claim_id,"claim_generation":prepared.claim_generation,
                "decision_digest":prepared.decision_digest,"canonical_subjects":prepared.canonical_subjects,
                "required_covers":prepared.required_covers,"observations":observations,
                "unestablished_obligations":missing
            }));
        }
        let unmapped: Vec<_> = replay
            .observations()
            .iter()
            .filter(|row| !mapped.contains(&row.subject.id()))
            .map(|row| row.subject.id())
            .collect();
        self.registry.require_registry_bytes()?;
        let scoreboard: serde_json::Value =
            formal::parse_json(self.registry.scoreboard_json()?.as_bytes())?;
        super::super::json(&serde_json::json!({
            "schema":"ripr.retained-native-consistency.v1","retained_consistency":"consistent",
            "registry_generation":self.registry.generation(),"bindings_digest":self.bindings_digest,
            "run_id":run.run_id,"claims":rows,"unmapped_native_subjects":unmapped,
            "canonical_scoreboard":scoreboard,"native_origin":"not_established",
            "native_assurance":"not_assessed","execution_authority":"not_granted",
            "limitations":["Self-consistent supplied or rehashed bytes do not prove that the named native process ran.",
                "The supplied runner/configuration/toolchain identity is not authenticated by replay.",
                "No live process capability or protected lifecycle/transition authority is created."]
        }))
    }
}

#[cfg(test)]
mod tests;

pub(crate) fn execute(args: &[String]) -> RegistryResult<String> {
    let mut values = BTreeMap::new();
    let mut flags = args.iter();
    while let Some(flag) = flags.next() {
        require(
            matches!(
                flag.as_str(),
                "--root" | "--bindings" | "--run" | "--artifacts"
            ),
            format!("unknown retained replay argument {flag}; {USAGE}"),
        )?;
        let value = flags
            .next()
            .ok_or_else(|| format!("{flag} requires a value; {USAGE}"))?;
        require(
            values.insert(flag.as_str(), value.as_str()).is_none(),
            format!("duplicate {flag}"),
        )?;
    }
    let value = |flag| {
        values
            .get(flag)
            .copied()
            .ok_or_else(|| format!("{flag} is required; {USAGE}"))
    };
    let registry = ValidatedRegistry::load(Path::new(value("--root")?))?;
    let bindings = BindingInput::parse(&native::read_file_bounded(
        Path::new(value("--bindings")?),
        super::super::MAX_INPUT_BYTES,
    )?)?;
    let run: formal::Run = formal::parse_json(&native::read_file_bounded(
        Path::new(value("--run")?),
        super::super::MAX_INPUT_BYTES,
    )?)?;
    registry
        .prepare_native(Some(&bindings))?
        .replay_native(&run, Path::new(value("--artifacts")?))
}
