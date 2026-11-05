use rust_catalog_description_structural_oracle::{compute,CalcError};
#[test]
fn observes() {
 assert_eq!(compute(-1).unwrap_err(),CalcError::Negative);
}
