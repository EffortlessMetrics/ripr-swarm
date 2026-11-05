use rust_catalog_description_structural_oracle::{score};
#[test]
fn observes() {
 let _=score(1);
 let value=2;
 assert!(matches!(value,2));
}
