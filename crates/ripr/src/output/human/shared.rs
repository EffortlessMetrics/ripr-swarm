//! #7394: attributable sharing in the navigation-aware exhaustive CLI report.
//! This is presentation only; the legacy library route never builds a catalogue.

use super::evidence_lines::evidence_path_entries;
use super::sections::{push_evidence_line, trim_field_period};
use crate::domain::Finding;
use crate::output::next_step::reconcile_next_step;
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct SharedReport {
    entries: Vec<Entry>,
    pub findings: Vec<SharedSections>,
}

struct Entry {
    label: Option<String>,
    members: Vec<usize>,
    text: String,
    guidance: bool,
}

#[derive(Default)]
pub(super) struct SharedSections {
    pub evidence: String,
    pub guidance: Option<String>,
}

impl SharedReport {
    #[cfg(test)]
    pub fn build(findings: &[&Finding]) -> Self {
        Self::build_with_origins(findings, None)
    }
    pub fn build_with_origins(
        findings: &[&Finding],
        origins: Option<&crate::value_origins::ValueOrigins>,
    ) -> Self {
        let mut report = Self::default();
        if findings
            .iter()
            .filter(|finding| finding.probe.owner.is_some())
            .take(2)
            .count()
            < 2
        {
            return report;
        }
        // Preserve the complete legacy layout for multiline literals. They can
        // resemble structural lines, so do not invent an escaping grammar or
        // collapse raw indentation. Inspect every raw Finding string (including
        // preview-card inputs); this temporary serialization is CLI-only and
        // retained nowhere. Serialization failure also refuses sharing.
        if findings.iter().any(|finding| {
            serde_json::to_string(finding).map_or(true, |raw| serialized_line_breaks(&raw))
        }) {
            return report;
        }
        let mut contexts = BTreeMap::new();
        let mut keys = BTreeMap::new();
        let mut records = Vec::new();
        for (ordinal, finding) in findings.iter().enumerate() {
            // Unknown owners cannot establish common ownership. Keep all four
            // currentness states separate, raw test rows/counts and full owner
            // identity (including modules/impls), rather than display captions.
            let context = finding.probe.owner.as_ref().and_then(|owner| {
                serde_json::to_vec(&(
                    &finding.probe.location.file,
                    owner,
                    finding.language,
                    finding.language_status,
                    finding.owner_kind,
                    finding.source_currentness,
                    &finding.related_tests,
                    finding.related_tests_matched_total,
                ))
                .ok()
            });
            let context = context.map(|key| {
                let next = contexts.len();
                *contexts.entry(key).or_insert(next)
            });
            let mut evidence = Vec::new();
            for entry in evidence_path_entries(
                finding,
                context.is_some(),
                origins.and_then(|origins| origins.files(finding)),
            ) {
                let shared = context.zip(entry.key).map(|(context, key)| {
                    report.intern(&mut keys, (context, false, key), ordinal, &entry.text)
                });
                evidence.push((entry.text, shared));
            }
            let guidance = finding.recommended_next_step.as_ref().map(|raw| {
                let text = reconcile_next_step(finding);
                let key = if text.contains(['\n', '\r']) {
                    None
                } else {
                    serde_json::to_vec(&(raw, &text)).ok()
                };
                let shared = context.zip(key).map(|(context, key)| {
                    report.intern(&mut keys, (context, true, key), ordinal, &text)
                });
                (text, shared)
            });
            records.push((evidence, guidance));
        }
        let mut evidence_number = 0;
        let mut guidance_number = 0;
        for entry in &mut report.entries {
            if entry.members.len() < 2 {
                continue;
            }
            let (prefix, number) = if entry.guidance {
                ("N", &mut guidance_number)
            } else {
                ("E", &mut evidence_number)
            };
            *number += 1;
            entry.label = Some(format!("{prefix}{number}"));
        }
        for (evidence, guidance) in records {
            let mut sections = SharedSections::default();
            let mut references = Vec::new();
            for (text, shared) in evidence {
                if let Some(label) = shared.and_then(|index| report.entries[index].label.as_ref()) {
                    references.push(label.as_str());
                } else {
                    flush_references(&mut sections.evidence, &mut references);
                    push_evidence_line(&mut sections.evidence, &text);
                }
            }
            flush_references(&mut sections.evidence, &mut references);
            sections.guidance = guidance.map(|(text, shared)| {
                if let Some(label) = shared.and_then(|index| report.entries[index].label.as_ref()) {
                    format!("  shared guidance: {label}\n")
                } else {
                    // #7756: the unshared per-finding guidance line is a
                    // list-item value, so it carries no trailing period
                    // either (Devin review on #8096).
                    format!("  - {}\n", trim_field_period(&text))
                }
            });
            report.findings.push(sections);
        }
        report
    }

    fn intern(
        &mut self,
        keys: &mut BTreeMap<(usize, bool, Vec<u8>), usize>,
        key: (usize, bool, Vec<u8>),
        ordinal: usize,
        text: &str,
    ) -> usize {
        let next = self.entries.len();
        let guidance = key.1;
        let index = *keys.entry(key).or_insert_with(|| {
            self.entries.push(Entry {
                label: None,
                members: Vec::new(),
                text: text.to_string(),
                guidance,
            });
            next
        });
        let members = &mut self.entries[index].members;
        if members.last() != Some(&ordinal) {
            members.push(ordinal);
        }
        index
    }

    pub fn is_shared(&self) -> bool {
        self.entries.iter().any(|entry| entry.label.is_some())
    }

    pub fn render_catalogue(&self, out: &mut String) {
        if !self.is_shared() {
            return;
        }
        out.push_str("Shared evidence and guidance\n");
        for entry in &self.entries {
            let Some(label) = &entry.label else { continue };
            let members = entry
                .members
                .iter()
                .map(|ordinal| (ordinal + 1).to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let mut payload = String::new();
            if entry.guidance {
                // #7756: catalogue guidance values follow the same
                // no-trailing-period rule as the per-finding render.
                payload.push_str(&format!("  {}\n", trim_field_period(&entry.text)));
            } else {
                push_evidence_line(&mut payload, &entry.text);
            }
            let prefix = if entry.guidance { "  " } else { "  - " };
            out.push_str(&format!(
                "  {label} [findings {members}]: {}",
                payload.strip_prefix(prefix).unwrap_or(&payload)
            ));
        }
        out.push('\n');
    }
}

fn flush_references(out: &mut String, references: &mut Vec<&str>) {
    if !references.is_empty() {
        out.push_str(&format!("  shared evidence: {}\n", references.join(", ")));
        references.clear();
    }
}

#[cfg(test)]
mod tests;

// Scan escapes rather than substrings: a JSON-escaped backslash consumes its
// second backslash, so ordinary source `\\n`/`\\r` and Windows paths stay
// eligible. Only actual LF/CR escapes (or serialization failure above) refuse.
fn serialized_line_breaks(raw: &str) -> bool {
    let mut escaped = false;
    for byte in raw.bytes() {
        if escaped {
            if matches!(byte, b'n' | b'r') {
                return true;
            }
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        }
    }
    false
}
