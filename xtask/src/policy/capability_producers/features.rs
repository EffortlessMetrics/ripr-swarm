//! Selected build contract for the producer inventory; no matrix execution.
use std::collections::BTreeSet;

use serde_json::Value;

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct BuildSelection {
    pub(super) no_default_features: bool,
    pub(super) features: BTreeSet<String>,
}

impl BuildSelection {
    pub(super) fn rerun_command(&self) -> String {
        let flags = self.cargo_args().into_iter().skip(4).collect::<Vec<_>>();
        let mut command = "cargo xtask check-capability-producers".to_string();
        if !flags.is_empty() {
            command.push(' ');
            command.push_str(&flags.join(" "));
        }
        command
    }

    pub(super) fn cargo_args(&self) -> Vec<String> {
        let mut args = ["build", "-p", "ripr", "--quiet"]
            .map(String::from)
            .to_vec();
        if self.no_default_features {
            args.push("--no-default-features".to_string());
        }
        if !self.features.is_empty() {
            args.extend([
                "--features".to_string(),
                self.features.iter().cloned().collect::<Vec<_>>().join(","),
            ]);
        }
        args
    }

    /// Resolve local feature edges from the actual manifest. Dependency edges
    /// cannot enable another local adapter. Reject unknown local selections
    /// rather than letting an offline registry certify a misspelled feature.
    pub(super) fn enabled_features(&self, manifest: &str) -> Result<BTreeSet<String>, String> {
        let cargo: toml::Value =
            toml::from_str(manifest).map_err(|err| format!("invalid ripr manifest: {err}"))?;
        let table = cargo
            .get("features")
            .and_then(toml::Value::as_table)
            .ok_or("ripr manifest has no features table")?;
        for feature in &self.features {
            if !feature.starts_with("lang-") || !table.contains_key(feature) {
                return Err(format!("unknown language feature `{feature}`"));
            }
        }
        let mut pending: Vec<String> = self.features.iter().cloned().collect();
        if !self.no_default_features {
            pending.push("default".to_string());
        }
        let mut visited = BTreeSet::new();
        while let Some(feature) = pending.pop() {
            if !visited.insert(feature.clone()) {
                continue;
            }
            let edges = table
                .get(&feature)
                .and_then(toml::Value::as_array)
                .ok_or_else(|| format!("feature `{feature}` has no feature array"))?;
            for edge in edges {
                let edge = edge
                    .as_str()
                    .ok_or_else(|| format!("feature `{feature}` has a non-string edge"))?;
                if !edge.starts_with("dep:") && !edge.contains('/') {
                    if table.contains_key(edge) {
                        pending.push(edge.to_string());
                    } else {
                        return Err(format!(
                            "feature `{feature}` references unknown local feature `{edge}`"
                        ));
                    }
                }
            }
        }
        // The product's lib.rs deliberately rejects Rust-disabled builds.
        // Saved JSON must not certify a feature combination it cannot build.
        if !visited.contains("lang-rust") {
            return Err("ripr requires lang-rust; the minimal supported selection is --no-default-features --features lang-rust".to_string());
        }
        Ok(visited
            .into_iter()
            .filter(|name| name.starts_with("lang-"))
            .collect())
    }
}

pub(super) fn compiled_feature_violations(
    registry: &Value,
    enabled: &BTreeSet<String>,
) -> Vec<String> {
    let mut violations = Vec::new();
    let Some(languages) = registry.get("languages").and_then(Value::as_array) else {
        return vec!["cannot check compiled producers without registry languages".to_string()];
    };
    for language in languages {
        let name = language
            .get("language")
            .and_then(Value::as_str)
            .unwrap_or("<missing>");
        let Some(feature) = language.get("required_feature").and_then(Value::as_str) else {
            violations.push(format!(
                "language `{name}` lacks required_feature for compiled producer check"
            ));
            continue;
        };
        // An independent inventory pin, aligned with LanguageId::required_feature.
        // A new routed language needs an explicit disposition here; saved JSON
        // cannot redefine an existing language's producer by swapping features.
        let canonical_feature = match name {
            "rust" => "lang-rust",
            "typescript" | "javascript" => "lang-typescript",
            "python" => "lang-python",
            "perl" => "lang-perl",
            _ => {
                violations.push(format!(
                    "language `{name}` has no canonical feature disposition"
                ));
                continue;
            }
        };
        if feature != canonical_feature {
            violations.push(format!(
                "language `{name}` required_feature must be `{canonical_feature}`, got `{feature}`"
            ));
            continue;
        }
        let expected = enabled.contains(feature);
        if language.get("adapter_compiled").and_then(Value::as_bool) != Some(expected) {
            violations.push(format!("language `{name}` adapter_compiled must be {expected} for required_feature `{feature}` in the selected build"));
        }
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::compiled_feature_violations;
    use serde_json::json;

    #[test]
    fn swapped_feature_identities_fail_despite_consistent_availability_flags() {
        let enabled = ["lang-rust".to_string()].into_iter().collect();
        let mut registry = json!({"languages": [
            {"language":"rust", "required_feature":"lang-rust", "adapter_compiled":true},
            {"language":"perl", "required_feature":"lang-perl", "adapter_compiled":false}
        ]});
        assert_eq!(
            compiled_feature_violations(&registry, &enabled),
            Vec::<String>::new()
        );
        registry["languages"][0]["required_feature"] = json!("lang-perl");
        registry["languages"][0]["adapter_compiled"] = json!(false);
        registry["languages"][1]["required_feature"] = json!("lang-rust");
        registry["languages"][1]["adapter_compiled"] = json!(true);
        assert_eq!(
            compiled_feature_violations(&registry, &enabled),
            [
                "language `rust` required_feature must be `lang-rust`, got `lang-perl`",
                "language `perl` required_feature must be `lang-perl`, got `lang-rust`"
            ]
        );
    }
}
