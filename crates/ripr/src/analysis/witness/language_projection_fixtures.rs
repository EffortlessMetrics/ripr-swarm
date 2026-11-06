pub(super) const FULLY_MAPPED: &str = r#"{
  "language": "python",
  "native_behavior_family": "predicate_boundary",
  "owner_identity": "module::discounted_total",
  "owner_source": "native_owner",
  "candidate_relations": [
    {
      "relation_reason": "direct_owner_call",
      "relation_confidence": "medium",
      "oracle_kind": "assertion",
      "oracle_strength": "strong",
      "status": "candidate",
      "has_test_target": true
    }
  ],
  "established_relations": [],
  "reach": { "state": "Yes", "confidence": "Medium", "summary": "producer" },
  "activation": { "state": "Yes", "confidence": "Medium", "summary": "producer" },
  "propagation": { "state": "Yes", "confidence": "Medium", "summary": "producer" },
  "observation": { "state": "Yes", "confidence": "Medium", "summary": "producer" },
  "discrimination": { "state": "Weak", "confidence": "Medium", "summary": "producer" },
  "required_discriminator": "amount >= threshold",
  "required_discriminator_source": "native",
  "observed_discriminators": [],
  "missing_discriminators": ["amount >= threshold"],
  "related_test_rows": [
    {
      "test_name": "test_discount_boundary",
      "row_assessment": "missing_assertion",
      "oracle_kind": "assertion",
      "oracle_strength": "strong",
      "status": "candidate"
    }
  ],
  "selected_target": {
    "kind": "typed_absence",
    "reason": "no_selected_target"
  },
  "limitations": [],
  "non_claims": [],
  "native_extensions": {
    "namespace": "python.preview.v1",
    "schema_version": 1,
    "fields": {}
  },
  "producer_identity": {
    "producer": "python_preview_adapter",
    "input": "diff_only_subject_set",
    "capability": "static_projection",
    "currentness": "current"
  }
}"#;

pub(super) const PARTIAL: &str = r#"{
  "language": "typescript",
  "native_behavior_family": "return_value",
  "owner_identity": "src/pricing.ts::total",
  "owner_source": "native_owner",
  "candidate_relations": [],
  "established_relations": [],
  "reach": { "state": "Yes", "confidence": "Medium", "summary": "producer" },
  "activation": { "state": "Yes", "confidence": "Medium", "summary": "producer" },
  "propagation": { "state": "Yes", "confidence": "Medium", "summary": "producer" },
  "observation": { "state": "Unknown", "confidence": "Unknown", "summary": "producer" },
  "discrimination": { "state": "Unknown", "confidence": "Unknown", "summary": "producer" },
  "required_discriminator": "return 42",
  "required_discriminator_source": "native",
  "observed_discriminators": [],
  "missing_discriminators": ["return 42"],
  "related_test_rows": [],
  "selected_target": {
    "kind": "typed_absence",
    "reason": "no_selected_target"
  },
  "limitations": ["missing_observer", "partial_projection_input"],
  "non_claims": [],
  "native_extensions": {
    "namespace": "typescript.preview.v1",
    "schema_version": 1,
    "fields": {}
  },
  "producer_identity": {
    "producer": "typescript_preview_adapter",
    "input": "partial_index",
    "capability": "static_projection",
    "currentness": "current"
  }
}"#;

pub(super) const UNMAPPABLE_NATIVE: &str = r#"{
  "language": "perl",
  "native_behavior_family": "perl.packet.mention_only",
  "owner_identity": "MyApp::Handler::run",
  "owner_source": "packet_owner",
  "candidate_relations": [],
  "established_relations": [],
  "reach": { "state": "Yes", "confidence": "Medium", "summary": "producer" },
  "activation": { "state": "Unknown", "confidence": "Unknown", "summary": "producer" },
  "propagation": { "state": "Unknown", "confidence": "Unknown", "summary": "producer" },
  "observation": { "state": "Unknown", "confidence": "Unknown", "summary": "producer" },
  "discrimination": { "state": "Unknown", "confidence": "Unknown", "summary": "producer" },
  "required_discriminator": "packet slot mention",
  "required_discriminator_source": "packet_fact",
  "observed_discriminators": [],
  "missing_discriminators": ["packet slot mention"],
  "related_test_rows": [],
  "selected_target": {
    "kind": "typed_absence",
    "reason": "no_selected_target"
  },
  "limitations": [],
  "non_claims": [],
  "native_extensions": {
    "namespace": "perl.preview.v1",
    "schema_version": 1,
    "fields": {
      "perl.packet.kind": "mention_only",
      "perl.packet.slot": "reserved"
    }
  },
  "producer_identity": {
    "producer": "perl_packet_adapter",
    "input": "packet_facts",
    "capability": "static_projection",
    "currentness": "current"
  }
}"#;

pub(super) const STALE_PRODUCER: &str = r#"{
  "language": "javascript",
  "native_behavior_family": "side_effect",
  "owner_identity": "src/io.js::writeConfig",
  "owner_source": "native_owner",
  "candidate_relations": [],
  "established_relations": [],
  "reach": { "state": "Yes", "confidence": "Medium", "summary": "producer" },
  "activation": { "state": "Yes", "confidence": "Medium", "summary": "producer" },
  "propagation": { "state": "Weak", "confidence": "Medium", "summary": "producer" },
  "observation": { "state": "Weak", "confidence": "Medium", "summary": "producer" },
  "discrimination": { "state": "Unknown", "confidence": "Unknown", "summary": "producer" },
  "required_discriminator": "fs.writeFileSync",
  "required_discriminator_source": "native",
  "observed_discriminators": [],
  "missing_discriminators": ["fs.writeFileSync"],
  "related_test_rows": [],
  "selected_target": {
    "kind": "typed_absence",
    "reason": "no_selected_target"
  },
  "limitations": [],
  "non_claims": [],
  "native_extensions": {
    "namespace": "javascript.preview.v1",
    "schema_version": 1,
    "fields": {}
  },
  "producer_identity": {
    "producer": "typescript_preview_adapter",
    "input": "workspace_complete",
    "capability": "static_projection",
    "currentness": "stale_input"
  }
}"#;
