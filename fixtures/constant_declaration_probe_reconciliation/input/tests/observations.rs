use constant_declaration_probe_fixture::OBSERVATION_SCHEMA_GENERATION;

#[test]
fn observation_schema_generation_matches_policy() {
    assert_eq!(OBSERVATION_SCHEMA_GENERATION, 3);
}
