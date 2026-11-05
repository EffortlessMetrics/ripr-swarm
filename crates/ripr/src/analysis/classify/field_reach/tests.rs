use super::*;
use crate::analysis::rust_index::summarize_file;
use std::path::PathBuf;

const SOURCE: &str = "pub struct Counter {
    count: u32,
    pub(crate) hits: u32,
    pub open: u32,
    pub(super) near: u32,
}

impl Counter {
    pub fn bump(&mut self) {
        self.count += 1;
    }
}

pub struct Twin {
    a: u32,
}
pub mod inner {
    pub struct Twin {
        a: u32,
    }
    impl Twin {
        pub fn set(&mut self) {
            self.a = 1;
        }
    }
}

pub struct Tuple(u32);
impl Tuple {
    pub fn set(&mut self) {
        self.0 = 1;
    }
}
";

fn reach_of(source: &str, owner_name: &str, field: &str) -> Option<FieldReach> {
    let path = PathBuf::from("src/lib.rs");
    let mut index = RustIndex::default();
    let facts = summarize_file(path.clone(), source.to_string());
    index.extend_functions(facts.functions.iter().cloned());
    index.insert_file_only(path, facts);
    let owner = index
        .functions()
        .iter()
        .find(|function| function.name == owner_name)?
        .clone();
    declared_field_reach(&owner, field, &index)
}

#[test]
fn a_declared_field_reads_its_own_visibility() {
    assert_eq!(reach_of(SOURCE, "bump", "count"), Some(FieldReach::Module));
    assert_eq!(reach_of(SOURCE, "bump", "hits"), Some(FieldReach::Crate));
    assert_eq!(reach_of(SOURCE, "bump", "open"), Some(FieldReach::Public));
    // A scoped restriction ripr does not model is read as private.
    assert_eq!(reach_of(SOURCE, "bump", "near"), Some(FieldReach::Module));
}

#[test]
fn an_undeclared_ambiguous_or_tuple_field_has_no_reach() {
    assert_eq!(reach_of(SOURCE, "bump", "missing"), None);
    assert_eq!(reach_of(SOURCE, "set", "a"), None);
    assert_eq!(reach_of(SOURCE, "set", "0"), None);
}

#[test]
fn a_test_names_a_field_only_where_its_visibility_reaches() {
    let owner = Path::new("crates/x/src/counter.rs");
    let inline = owner;
    let sibling = Path::new("crates/x/src/counter/tests.rs");
    let other_module = Path::new("crates/x/src/other.rs");
    let integration = Path::new("crates/x/tests/counter.rs");
    let other_crate = Path::new("crates/y/src/lib.rs");
    let binary = Path::new("crates/x/src/bin/tool.rs");

    for reach in [FieldReach::Public, FieldReach::Crate, FieldReach::Module] {
        assert!(test_can_name_field(reach, owner, inline), "{reach:?}");
    }
    assert!(test_can_name_field(FieldReach::Public, owner, integration));
    assert!(test_can_name_field(FieldReach::Crate, owner, sibling));
    assert!(test_can_name_field(FieldReach::Crate, owner, other_module));
    assert!(!test_can_name_field(FieldReach::Crate, owner, integration));
    assert!(!test_can_name_field(FieldReach::Crate, owner, other_crate));
    assert!(!test_can_name_field(FieldReach::Crate, owner, binary));
    assert!(!test_can_name_field(FieldReach::Module, owner, sibling));
    assert!(!test_can_name_field(FieldReach::Module, owner, integration));
    // A binary owner is its own crate: only its own file reads its fields.
    assert!(!test_can_name_field(
        FieldReach::Crate,
        binary,
        other_module
    ));
}

#[test]
fn mentions_count_every_use_of_the_name_but_not_its_definition() {
    let text = "fn bump(&mut self) {}\nfn t() {\n    c.bump();\n    C::bump(&mut c);\n    v.for_each(C::bump);\n    c.bump::<u8>();\n    let bumper = 1;\n}\n";
    assert_eq!(mentions_in(text, "bump"), 4);
    assert_eq!(mentions_in(text, "bumper"), 1);
    assert_eq!(mentions_in(text, "missing"), 0);
}
