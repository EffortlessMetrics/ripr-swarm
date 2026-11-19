//! v2 fact projection into common RIPR evidence (#3221).

mod dispositions;
mod findings;
mod sink_alignment;

#[cfg(test)]
pub(super) use dispositions::{
    behavior_hint_to_probe_family, perl_oracle_kind_to_domain, perl_oracle_strength_to_domain,
    perl_relation_to_domain,
};
pub(super) use findings::{
    change_lines_from_diff, packet_to_findings, packet_to_findings_with_currentness,
    perl_change_currentness, unowned_change_findings,
};
#[cfg(test)]
pub(super) use sink_alignment::sink_text_eq;
