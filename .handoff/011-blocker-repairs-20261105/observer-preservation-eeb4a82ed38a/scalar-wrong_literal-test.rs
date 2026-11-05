use rust_catalog_description_structural_oracle::{score};
#[test]
fn observes() {
 let value=score(1);
 assert!(matches!(value,5));
}
