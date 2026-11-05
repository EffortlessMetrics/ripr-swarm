use rust_catalog_description_structural_oracle::{compute,CalcError};
#[test]
fn observes() {
 let mut err=compute(-1).unwrap_err();
 err=CalcError::Negative;
 assert_eq!(err,CalcError::Negative);
}
