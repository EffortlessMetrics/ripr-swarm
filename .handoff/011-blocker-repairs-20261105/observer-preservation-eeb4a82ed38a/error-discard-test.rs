use rust_catalog_description_structural_oracle::{compute,CalcError};
#[test]
fn observes() {
 let _=compute(-1);
 let err=CalcError::Negative;
 assert_eq!(err,CalcError::Negative);
}
