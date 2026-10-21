//! Drift gate between the capability table and `docs/status/SUPPORT_TIERS.md`.
//!
//! The table is the machine authority for what each language/framework/mode
//! can establish; the support map is the human tier statement. This gate keeps
//! the two from diverging in either direction: a documented tier must equal
//! the ceiling of every cell bound to its row, and every cell must be bound to
//! exactly one documented row, so a new cell cannot appear without an explicit
//! documented disposition. It never promotes a tier.

use super::LanguageId;
use super::language_capability::{CAPABILITY_CELLS, CapabilityCell, CapabilityFamily, TierCeiling};

/// One support-map row and the cells it governs.
struct Binding {
    row: &'static str,
    languages: &'static [LanguageId],
    family: Option<CapabilityFamily>,
}

const BINDINGS: &[Binding] = &[
    Binding {
        row: "Rust static exposure loop",
        languages: &[LanguageId::Rust],
        family: Some(CapabilityFamily::StaticFacts),
    },
    Binding {
        row: "Rust gap repair loop",
        languages: &[LanguageId::Rust],
        family: Some(CapabilityFamily::RepairRouting),
    },
    Binding {
        row: "TypeScript and JavaScript preview",
        languages: &[LanguageId::TypeScript, LanguageId::JavaScript],
        family: None,
    },
    Binding {
        row: "Python repair routing",
        languages: &[LanguageId::Python],
        family: Some(CapabilityFamily::RepairRouting),
    },
    Binding {
        row: "Python preview static facts",
        languages: &[LanguageId::Python],
        family: Some(CapabilityFamily::StaticFacts),
    },
    Binding {
        row: "Perl repair routing",
        languages: &[LanguageId::Perl],
        family: None,
    },
];

impl Binding {
    fn governs(&self, cell: &CapabilityCell) -> bool {
        self.languages.contains(&cell.language)
            && self.family.is_none_or(|family| family == cell.family)
    }
}

fn cell_id(cell: &CapabilityCell) -> String {
    format!(
        "{}/{}/{}/{}",
        cell.language.as_str(),
        cell.framework,
        cell.mode.as_str(),
        cell.family.as_str()
    )
}

/// Rows of the "Current Support Map" table as (capability, tier text).
fn support_map(doc: &str) -> Vec<(String, String)> {
    let mut in_section = false;
    let mut rows = Vec::new();
    for line in doc.lines() {
        if line.starts_with("## ") {
            in_section = line.trim() == "## Current Support Map";
            continue;
        }
        if !in_section || !line.starts_with('|') {
            continue;
        }
        let mut columns = line.trim().trim_matches('|').split('|');
        let (Some(name), Some(tier)) = (columns.next(), columns.next()) else {
            continue;
        };
        let name = name.trim();
        if name == "Capability" || name.starts_with("---") {
            continue;
        }
        rows.push((name.to_string(), tier.trim().trim_matches('`').to_string()));
    }
    rows
}

fn documented_tier(text: &str) -> Option<TierCeiling> {
    match text {
        "usable alpha" => Some(TierCeiling::UsableAlpha),
        "preview" => Some(TierCeiling::Preview),
        _ => None,
    }
}

/// Every way the table and the support map disagree, one sentence each.
fn drift_violations(doc: &str, cells: &[CapabilityCell], bindings: &[&Binding]) -> Vec<String> {
    let rows = support_map(doc);
    let mut violations = Vec::new();
    for binding in bindings {
        let Some((_, tier_text)) = rows.iter().find(|(name, _)| name == binding.row) else {
            violations.push(format!(
                "support map has no row `{}` for its bound cells",
                binding.row
            ));
            continue;
        };
        let Some(tier) = documented_tier(tier_text) else {
            violations.push(format!(
                "row `{}` has tier `{tier_text}`, which no capability cell can carry",
                binding.row
            ));
            continue;
        };
        let governed: Vec<&CapabilityCell> =
            cells.iter().filter(|cell| binding.governs(cell)).collect();
        if governed.is_empty() {
            violations.push(format!("row `{}` governs no capability cell", binding.row));
        }
        for cell in governed {
            if cell.tier_ceiling != tier {
                violations.push(format!(
                    "cell {} has ceiling {} but row `{}` documents {}",
                    cell_id(cell),
                    cell.tier_ceiling.as_str(),
                    binding.row,
                    tier.as_str()
                ));
            }
        }
    }
    for cell in cells {
        let bound = bindings
            .iter()
            .filter(|binding| binding.governs(cell))
            .count();
        if bound != 1 {
            violations.push(format!(
                "cell {} is bound to {bound} support-map rows, expected exactly 1",
                cell_id(cell)
            ));
        }
    }
    violations
}

fn current_support_tiers() -> String {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/status/SUPPORT_TIERS.md");
    std::fs::read_to_string(&path).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_bindings() -> Vec<&'static Binding> {
        BINDINGS.iter().collect()
    }

    #[test]
    fn the_support_map_has_rows_to_check() {
        assert!(
            support_map(&current_support_tiers()).len() >= BINDINGS.len(),
            "SUPPORT_TIERS.md Current Support Map was not read"
        );
    }

    #[test]
    fn the_table_agrees_with_the_support_map() {
        assert_eq!(
            drift_violations(&current_support_tiers(), CAPABILITY_CELLS, &all_bindings()),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_documented_tier_below_the_ceiling_is_drift() {
        let doc = current_support_tiers().replacen(
            "| Python repair routing | `usable alpha` |",
            "| Python repair routing | `preview` |",
            1,
        );
        let violations = drift_violations(&doc, CAPABILITY_CELLS, &all_bindings());
        assert!(
            violations
                .iter()
                .any(|v| v.contains("python/pytest/diff/repair_routing")),
            "{violations:?}"
        );
    }

    #[test]
    fn a_documented_tier_above_the_ceiling_is_drift() {
        let doc = current_support_tiers().replacen(
            "| Perl repair routing | `preview` |",
            "| Perl repair routing | `usable alpha` |",
            1,
        );
        let violations = drift_violations(&doc, CAPABILITY_CELLS, &all_bindings());
        assert!(
            violations.iter().any(|v| v.contains("perl/any/")),
            "{violations:?}"
        );
    }

    #[test]
    fn a_removed_row_is_drift() {
        let doc: String = current_support_tiers()
            .lines()
            .filter(|line| !line.starts_with("| Rust gap repair loop |"))
            .collect::<Vec<_>>()
            .join("\n");
        let violations = drift_violations(&doc, CAPABILITY_CELLS, &all_bindings());
        assert!(
            violations
                .iter()
                .any(|v| v.contains("Rust gap repair loop")),
            "{violations:?}"
        );
    }

    #[test]
    fn a_cell_without_a_documented_row_is_drift() {
        let without_perl: Vec<&Binding> = BINDINGS
            .iter()
            .filter(|binding| binding.row != "Perl repair routing")
            .collect();
        let violations =
            drift_violations(&current_support_tiers(), CAPABILITY_CELLS, &without_perl);
        assert!(
            violations
                .iter()
                .any(|v| v.contains("perl/any/diff/static_facts")
                    && v.contains("0 support-map rows")),
            "{violations:?}"
        );
    }
}
