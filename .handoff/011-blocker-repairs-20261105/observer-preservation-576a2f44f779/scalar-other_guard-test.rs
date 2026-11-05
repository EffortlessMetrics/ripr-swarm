use rust_catalog_description_structural_oracle::{score};
#[test]
fn observes() {
 let value=score(1);
 let other=2;
 assert!(matches!(value,_ if other==2));
}
