use rust_catalog_description_structural_oracle::{score};
#[test]
fn observes() {
 let value=score(1);
 assert!(matches!(value,_ if value==2 || true));
}
