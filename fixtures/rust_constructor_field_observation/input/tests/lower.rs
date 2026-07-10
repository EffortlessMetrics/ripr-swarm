use rust_constructor_field_observation_fixture::{HirLet, Storage, lower_ast};

#[test]
fn lower_ast_preserves_storage() {
    let HirLet { storage, .. } = lower_ast("item".to_string(), Storage::Our);
    assert_eq!(storage, Storage::Our);
}
