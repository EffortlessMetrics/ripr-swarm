//! Compile the ordinary route-parity contract as an external crate consumer.
use repo_policy::product_gate_plan::{
    ProductGateId, ProductGateParity, ProductGatePlan, ProductGateRole, ProductGateSelection,
    ProductGateSubject, ProductGateTrustClass, ProductSurface, product_gate_definitions,
};

#[test]
fn route_parity_consumes_the_single_canonical_portfolio() {
    let definitions = product_gate_definitions();
    assert_eq!(
        definitions.len(),
        13,
        "ordinary portfolio must not silently become the staged DAG"
    );
    let ids = definitions.iter().map(|row| row.id).collect();
    assert!(ProductGateParity::compare(ids).aligned);
    let plan = ProductGatePlan::for_subject(&ProductGateSubject {
        changed_surfaces: [ProductSurface::Rust].into_iter().collect(),
        selectors_authoritative: true,
        trust_class: ProductGateTrustClass::Repository,
    });
    assert_eq!(plan.rows.len(), 13);
    assert!(plan.full_route_reason.is_none());
    assert!(
        plan.rows
            .iter()
            .any(|row| row.definition.id == ProductGateId::Clippy
                && row.selection == ProductGateSelection::Selected
                && row.definition.role == ProductGateRole::Required)
    );
    let missing = ProductGateParity::compare(
        definitions
            .into_iter()
            .filter(|row| row.id != ProductGateId::Clippy)
            .map(|row| row.id)
            .collect(),
    );
    assert!(!missing.aligned);
    assert!(
        missing
            .missing_from_producer
            .contains("product.rust.clippy")
    );
}

#[test]
fn route_parity_preserves_upstream_enum_wire_names() -> Result<(), String> {
    for (actual, expected) in [
        (serde_json::to_value(ProductGateRole::Required), "Required"),
        (
            serde_json::to_value(ProductGateSelection::NotProven),
            "NotProven",
        ),
        (
            serde_json::to_value(ProductGateTrustClass::ExternalTree),
            "ExternalTree",
        ),
    ] {
        assert_eq!(actual.map_err(|e| e.to_string())?, expected);
    }
    let role: ProductGateRole = serde_json::from_str("\"Required\"").map_err(|e| e.to_string())?;
    let selection: ProductGateSelection =
        serde_json::from_str("\"NotProven\"").map_err(|e| e.to_string())?;
    let trust: ProductGateTrustClass =
        serde_json::from_str("\"ExternalTree\"").map_err(|e| e.to_string())?;
    assert_eq!(
        (role, selection, trust),
        (
            ProductGateRole::Required,
            ProductGateSelection::NotProven,
            ProductGateTrustClass::ExternalTree
        )
    );
    Ok(())
}
