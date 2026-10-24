//! Drift gate between the capability table and `docs/status/SUPPORT_TIERS.md`.
//!
//! The table is the machine authority for what each language/framework/mode
//! can establish; the support map is the human tier statement. This gate keeps
//! the two from diverging in either direction: a documented tier must equal
//! the ceiling of every cell bound to its row, and every cell must be bound to
//! exactly one documented row, so a new cell cannot appear without an explicit
//! documented disposition. It never promotes a tier.

use super::language_capability::{
    CAPABILITY_CELLS, CapabilityCell, REGISTRY_END, SupportState, TierCeiling, cell_id,
    render_registry_block,
};

/// One support-map row and the explicit cell identities it governs. A cell is
/// never bound by pattern, so a new cell needs a deliberate documented row.
struct Binding {
    row: &'static str,
    cells: &'static [&'static str],
}

const BINDINGS: &[Binding] = &[
    Binding {
        row: "Rust static exposure loop",
        cells: &["rust/any/diff/static_facts", "rust/any/repo/static_facts"],
    },
    Binding {
        row: "Rust gap repair loop",
        cells: &["rust/any/diff/repair_routing"],
    },
    Binding {
        row: "TypeScript and JavaScript preview",
        cells: &[
            "typescript/any/diff/static_facts",
            "typescript/any/repo/static_facts",
            "typescript/any/diff/repair_routing",
            "javascript/any/diff/static_facts",
            "javascript/any/repo/static_facts",
            "javascript/any/diff/repair_routing",
        ],
    },
    Binding {
        row: "Python repair routing",
        cells: &[
            "python/pytest/diff/repair_routing",
            "python/unittest/diff/repair_routing",
        ],
    },
    Binding {
        row: "Python preview static facts",
        cells: &[
            "python/any/diff/static_facts",
            "python/any/repo/static_facts",
        ],
    },
    Binding {
        row: "Perl repair routing",
        cells: &[
            "perl/any/diff/static_facts",
            "perl/any/repo/static_facts",
            "perl/any/diff/repair_routing",
        ],
    },
];

impl Binding {
    fn governs(&self, cell: &CapabilityCell) -> bool {
        self.cells.contains(&cell_id(cell).as_str())
    }
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
        let matching: Vec<&(String, String)> = rows
            .iter()
            .filter(|(name, _)| name == binding.row)
            .collect();
        let [(_, tier_text)] = matching.as_slice() else {
            violations.push(format!(
                "support map has {} rows named `{}`, expected exactly 1",
                matching.len(),
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
        for id in binding.cells {
            if !cells.iter().any(|cell| cell_id(cell) == *id) {
                violations.push(format!(
                    "row `{}` binds `{id}`, which is not a capability cell",
                    binding.row
                ));
            }
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

/// The marker block as written in the doc, or why it cannot be read.
fn documented_registry_block(doc: &str) -> Result<&str, String> {
    let begin = doc.matches("<!-- capability-registry:begin").count();
    let end = doc.matches(REGISTRY_END).count();
    if begin != 1 || end != 1 {
        return Err(format!(
            "expected exactly one capability-registry begin/end marker pair, found {begin}/{end}"
        ));
    }
    let start = doc
        .find("<!-- capability-registry:begin")
        .ok_or_else(|| "begin marker missing".to_string())?;
    let stop = doc
        .find(REGISTRY_END)
        .ok_or_else(|| "end marker missing".to_string())?
        + REGISTRY_END.len();
    doc.get(start..stop)
        .filter(|_| start < stop)
        .ok_or_else(|| "end marker precedes begin marker".to_string())
}

/// Empty when the documented block equals the rendering of the table.
fn registry_block_violations(doc: &str, cells: &[CapabilityCell], path: &str) -> Vec<String> {
    let expected = render_registry_block(cells);
    match documented_registry_block(doc) {
        Err(reason) => vec![reason],
        Ok(actual) if actual == expected => Vec::new(),
        Ok(_) => vec![format!(
            "the generated capability-registry block in {path} is stale; replace it with:\n{expected}"
        )],
    }
}

/// Cell identities the preview guide's prose says are `not_proven`, as
/// `<cell id> is not_proven`. The prose may only say this of a cell the
/// registry lacks or holds at `not_proven`.
fn prose_not_proven_claims(prose: &str) -> Vec<String> {
    const CLAIM: &str = " is not_proven";
    let mut claims = Vec::new();
    for (at, _) in prose.match_indices(CLAIM) {
        let head = prose.get(..at).unwrap_or_default();
        let start = head
            .rfind(|c: char| {
                !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '/')
            })
            .map_or(0, |i| i + 1);
        claims.push(head.get(start..).unwrap_or_default().to_string());
    }
    claims
}

/// Every way the preview guide's registry-owned statements disagree with the
/// registry: a stale or missing generated block, a tier word the block owns
/// written into the prose, or a `not_proven` claim about a cell the registry
/// holds at another state. Narrative prose that names no per-cell state is not
/// inspected.
fn preview_guide_violations(doc: &str, cells: &[CapabilityCell]) -> Vec<String> {
    let mut violations = registry_block_violations(doc, cells, PREVIEW_GUIDE);
    let Some(prose) = documented_registry_block(doc)
        .ok()
        .and_then(|block| doc.replacen(block, "", 1).into())
    else {
        return violations;
    };
    // Hard-wrapped prose must not hide a claim across a line break.
    let prose = prose.split_whitespace().collect::<Vec<_>>().join(" ");
    let lowered = prose.to_ascii_lowercase();
    if lowered.contains("usable alpha") || lowered.contains("usable_alpha") {
        violations.push(format!(
            "{PREVIEW_GUIDE} prose states a `usable alpha` tier; per-cell tiers belong to the generated registry block only"
        ));
    }
    for claim in prose_not_proven_claims(&prose) {
        let held = cells.iter().find(|cell| cell_id(cell) == claim);
        if let Some(cell) = held.filter(|cell| cell.state != SupportState::NotProven) {
            violations.push(format!(
                "{PREVIEW_GUIDE} says `{claim} is not_proven` but the registry holds it at {}",
                cell.state.as_str()
            ));
        }
    }
    violations
}

const PREVIEW_GUIDE: &str = "docs/LANGUAGE_ADAPTER_PREVIEW.md";

fn current_preview_guide() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/LANGUAGE_ADAPTER_PREVIEW.md");
    std::fs::read_to_string(&path).unwrap_or_default()
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
    fn the_generated_registry_block_matches_the_table() {
        assert_eq!(
            registry_block_violations(
                &current_support_tiers(),
                CAPABILITY_CELLS,
                "docs/status/SUPPORT_TIERS.md"
            ),
            Vec::<String>::new()
        );
    }

    fn synthetic_doc() -> String {
        format!(
            "# Doc\n\nprose\n\n{}\n\nmore prose\n",
            render_registry_block(CAPABILITY_CELLS)
        )
    }

    #[test]
    fn a_changed_cell_state_makes_the_registry_block_stale() {
        let doc = synthetic_doc();
        assert_eq!(
            registry_block_violations(&doc, CAPABILITY_CELLS, "doc"),
            Vec::<String>::new()
        );
        let mut cells = CAPABILITY_CELLS.to_vec();
        cells[0].state = super::super::language_capability::SupportState::Stale;
        assert_ne!(cells[0].state, CAPABILITY_CELLS[0].state);
        let violations = registry_block_violations(&doc, &cells, "doc");
        assert!(
            violations.iter().any(|v| v.contains("stale")),
            "{violations:?}"
        );
    }

    #[test]
    fn a_hand_edited_or_missing_registry_block_is_drift() {
        let doc = synthetic_doc();
        let edited = doc.replacen("| `supported", "| `edited", 1);
        assert_ne!(edited, doc);
        assert!(
            registry_block_violations(&edited, CAPABILITY_CELLS, "doc")
                .iter()
                .any(|v| v.contains("stale"))
        );
        let removed = doc.replace(REGISTRY_END, "");
        assert_ne!(removed, doc);
        assert!(
            registry_block_violations(&removed, CAPABILITY_CELLS, "doc")
                .iter()
                .any(|v| v.contains("marker"))
        );
    }

    #[test]
    fn the_preview_guide_agrees_with_the_registry() {
        assert_eq!(
            preview_guide_violations(&current_preview_guide(), CAPABILITY_CELLS),
            Vec::<String>::new()
        );
    }

    #[test]
    fn the_preview_guide_not_proven_claims_are_read() {
        let doc = current_preview_guide();
        let claims = prose_not_proven_claims(&doc.split_whitespace().collect::<Vec<_>>().join(" "));
        assert!(
            claims.contains(&"python/any/repo/repair_routing".to_string())
                && claims.contains(&"typescript/any/repo/repair_routing".to_string()),
            "{claims:?}"
        );
    }

    #[test]
    fn a_hand_edited_preview_guide_block_is_drift() {
        let doc = current_preview_guide();
        let edited = doc.replacen("| `preview` |", "| `usable_alpha` |", 1);
        assert_ne!(edited, doc);
        let violations = preview_guide_violations(&edited, CAPABILITY_CELLS);
        assert!(
            violations.iter().any(|v| v.contains("stale")),
            "{violations:?}"
        );
        let removed = doc.replace(REGISTRY_END, "");
        assert!(
            preview_guide_violations(&removed, CAPABILITY_CELLS)
                .iter()
                .any(|v| v.contains("marker"))
        );
    }

    #[test]
    fn preview_guide_prose_contradicting_the_registry_is_drift() {
        let doc = current_preview_guide();
        // Claims a cell the registry supports is not_proven.
        let wrong_state = doc.replacen(
            "typescript/any/repo/repair_routing is not_proven",
            "typescript/any/diff/repair_routing is not_proven",
            1,
        );
        assert_ne!(wrong_state, doc);
        assert!(
            preview_guide_violations(&wrong_state, CAPABILITY_CELLS)
                .iter()
                .any(|v| v.contains("typescript/any/diff/repair_routing is not_proven"))
        );
        // Re-introduces a hand-written per-cell tier.
        let tier = doc.replacen(
            "scoped repair-routing lane",
            "scoped `usable alpha` lane",
            1,
        );
        assert_ne!(tier, doc);
        assert!(
            preview_guide_violations(&tier, CAPABILITY_CELLS)
                .iter()
                .any(|v| v.contains("usable alpha"))
        );
        // The same prose is consistent against a registry where that cell
        // really is absent (an unclaimed cell is not a contradiction).
        let without: Vec<CapabilityCell> = CAPABILITY_CELLS
            .iter()
            .filter(|c| cell_id(c) != "typescript/any/diff/repair_routing")
            .cloned()
            .collect();
        assert!(
            !preview_guide_violations(&wrong_state, &without)
                .iter()
                .any(|v| v.contains("is not_proven"))
        );
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
    fn a_duplicate_support_row_is_drift() {
        let with_dup = current_support_tiers().replacen(
            "| Python repair routing | `usable alpha` |",
            "| Python repair routing | `usable alpha` |\n| Python repair routing | `preview` |",
            1,
        );
        assert_ne!(with_dup, current_support_tiers());
        let violations = drift_violations(&with_dup, CAPABILITY_CELLS, &all_bindings());
        assert!(
            violations
                .iter()
                .any(|v| v.contains("2 rows named `Python repair routing`")),
            "{violations:?}"
        );
    }

    #[test]
    fn a_cell_bound_to_two_rows_is_drift() {
        let extra = Binding {
            row: "Rust gap repair loop",
            cells: &["perl/any/diff/static_facts"],
        };
        let mut bindings = all_bindings();
        bindings.push(&extra);
        let violations = drift_violations(&current_support_tiers(), CAPABILITY_CELLS, &bindings);
        assert!(
            violations
                .iter()
                .any(|v| v.contains("perl/any/diff/static_facts") && v.contains("2 support-map")),
            "{violations:?}"
        );
    }

    #[test]
    fn a_binding_to_an_unknown_cell_is_drift() {
        let extra = Binding {
            row: "Perl repair routing",
            cells: &["perl/any/diff/nonexistent"],
        };
        let mut bindings = all_bindings();
        bindings.push(&extra);
        let violations = drift_violations(&current_support_tiers(), CAPABILITY_CELLS, &bindings);
        assert!(
            violations
                .iter()
                .any(|v| v.contains("not a capability cell")),
            "{violations:?}"
        );
    }

    #[test]
    fn an_unknown_tier_word_is_drift() {
        let doc = current_support_tiers().replacen(
            "| Perl repair routing | `preview` |",
            "| Perl repair routing | `experimental` |",
            1,
        );
        let violations = drift_violations(&doc, CAPABILITY_CELLS, &all_bindings());
        assert!(
            violations.iter().any(|v| v.contains("`experimental`")),
            "{violations:?}"
        );
    }

    #[test]
    fn a_new_cell_without_an_explicit_binding_is_drift() {
        let mut cells = CAPABILITY_CELLS.to_vec();
        let mut extra = cells[0];
        extra.framework = "vitest";
        extra.language = super::super::LanguageId::TypeScript;
        cells.push(extra);
        let violations = drift_violations(&current_support_tiers(), &cells, &all_bindings());
        assert!(
            violations
                .iter()
                .any(|v| v.contains("typescript/vitest/") && v.contains("0 support-map")),
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
