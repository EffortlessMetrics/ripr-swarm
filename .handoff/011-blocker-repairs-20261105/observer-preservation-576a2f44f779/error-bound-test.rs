use rust_catalog_description_structural_oracle::{compute,CalcError};
#[test]
fn observes() {
 let err=compute(-1).unwrap_err();
 assert_eq!(err,CalcError::Negative);
}
