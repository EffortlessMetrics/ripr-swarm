use rust_catalog_description_structural_oracle::{score};
#[test]
fn observes() {
 let value={let _=score(1);2};
 assert!(matches!(value,2));
}
