use super::*;
use crate::analysis::rust_index::summarize_file;
use crate::domain::{DeltaKind, OracleStrength, ProbeId, SourceLocation, SymbolId};
use std::path::{Path, PathBuf};

const LIB: &str = "src/lib.rs";

/// The verdict-corpus `ledger-receive-refresh-low-stock` subject after its
/// diff: `receive` gained `self.refresh_low_stock(sku);`, which writes only
/// `self.low_stock`, read only by `is_low`.
const LEDGER: &str = r#"use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Received { sku: String, qty: u32 },
    Shipped { sku: String, qty: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    pub sku: String,
    pub qty: u32,
    pub remaining: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Inventory {
    stock: BTreeMap<String, u32>,
    log: Vec<Event>,
    shipped_total: u64,
    low_stock: BTreeSet<String>,
    low_threshold: u32,
}

impl Inventory {
    pub fn new(low_threshold: u32) -> Self {
        Self {
            low_threshold,
            ..Self::default()
        }
    }

    pub fn receive(&mut self, sku: &str, qty: u32) {
        *self.stock.entry(sku.to_string()).or_insert(0) += qty;
        self.log.push(Event::Received { sku: sku.to_string(), qty });
        self.refresh_low_stock(sku);
    }

    pub fn ship(&mut self, sku: &str, qty: u32) -> Result<Receipt, String> {
        let available = match self.stock.get(sku) {
            Some(n) => *n,
            None => return Err(sku.to_string()),
        };
        let remaining = available - qty;
        self.stock.insert(sku.to_string(), remaining);
        self.shipped_total += u64::from(qty);
        self.log.push(Event::Shipped { sku: sku.to_string(), qty });
        self.refresh_low_stock(sku);
        Ok(Receipt {
            sku: sku.to_string(),
            qty,
            remaining,
        })
    }

    fn refresh_low_stock(&mut self, sku: &str) {
        if self.on_hand(sku) < self.low_threshold {
            self.low_stock.insert(sku.to_string());
        } else {
            self.low_stock.remove(sku);
        }
    }

    pub fn on_hand(&self, sku: &str) -> u32 {
        self.stock.get(sku).copied().unwrap_or(0)
    }

    pub fn history(&self) -> &[Event] {
        &self.log
    }

    pub fn is_low(&self, sku: &str) -> bool {
        self.low_stock.contains(sku)
    }

    pub fn low_skus(&self) -> Vec<String> {
        self.low_stock.iter().cloned().collect()
    }
}
"#;

const LEDGER_TESTS: &str = r#"use demo::*;

#[test]
fn ledger() {
    let mut inv = Inventory::new(5);
    inv.receive("BOLT-M8", 10);
    let receipt = inv.ship("BOLT-M8", 3).unwrap();
    let copy = inv.clone();
    let low = inv.low_skus();
    let fresh = Inventory::new(5);
    assert_eq!(inv.history(), &[Event::Received { sku: "BOLT-M8".to_string(), qty: 10 }]);
    assert_eq!(receipt, Receipt { sku: "BOLT-M8".to_string(), qty: 3, remaining: 7 });
    assert_eq!(inv.history().to_vec(), Vec::new());
    assert_eq!(inv.low_skus(), vec!["BOLT-M8".to_string()]);
    assert_eq!(vec![inv.is_low("BOLT-M8")], vec![true]);
    assert_eq!(inv, fresh);
    assert_eq!(copy, fresh);
    assert_eq!(low, Vec::<String>::new());
    assert_eq!(unbound, Receipt { sku: "X".to_string(), qty: 1, remaining: 0 });
    assert_eq!(format!("{:?}", inv), String::new());
}
"#;

fn index(files: &[(&str, &str)]) -> RustIndex {
    let mut index = RustIndex::default();
    index.package_names.insert("demo".to_string());
    for (path, text) in files {
        let facts = summarize_file(PathBuf::from(path), (*text).to_string());
        index.extend_functions(facts.functions.iter().cloned());
        index.extend_tests(facts.tests.iter().cloned());
        index.insert_file_only(PathBuf::from(path), facts);
    }
    index
}

fn owner<'a>(index: &'a RustIndex, name: &str) -> &'a FunctionSummary {
    let found = index
        .functions()
        .iter()
        .find(|function| function.name == name && function.file == Path::new(LIB));
    assert!(found.is_some(), "owner `{name}` must be indexed from {LIB}");
    found.unwrap_or(index.functions().at(0))
}

fn effect_probe(owner: &FunctionSummary, expression: &str) -> Probe {
    Probe {
        id: ProbeId("probe".to_string()),
        location: SourceLocation::new(owner.file.clone(), owner.start_line + 3, 1),
        owner: Some(SymbolId(owner.id.0.clone())),
        family: ProbeFamily::CallDeletion,
        delta: DeltaKind::Effect,
        before: None,
        after: None,
        expression: expression.to_string(),
        expected_sinks: Vec::new(),
        required_oracles: Vec::new(),
    }
}

fn establish(index: &RustIndex, expression: &str) -> Option<EffectStateCarrier> {
    let owner = owner(index, "receive");
    EffectStateCarrier::establish(&effect_probe(owner, expression), owner, index)
}

fn whole_object(text: &str) -> OracleFact {
    OracleFact {
        line: 1,
        text: text.to_string(),
        kind: OracleKind::WholeObjectEquality,
        strength: OracleStrength::Strong,
        observed_tokens: Vec::new(),
        ok_value_observed: None,
    }
}

fn the_test(index: &RustIndex) -> &TestSummary {
    assert_eq!(index.tests().len(), 1, "fixture must hold exactly one test");
    index.tests().at(0)
}

#[test]
fn ledger_carrier_bounds_written_fields_and_readers() {
    let index = index(&[(LIB, LEDGER)]);
    let carrier = establish(&index, "self.refresh_low_stock(sku)");
    assert!(
        carrier.is_some(),
        "the ledger shape must establish a carrier"
    );
    let Some(carrier) = carrier else { return };
    assert_eq!(
        carrier.written_fields,
        BTreeSet::from(["low_stock".to_string()])
    );
    assert_eq!(
        carrier.reader_methods,
        BTreeSet::from(["is_low".to_string(), "low_skus".to_string()])
    );
    for non_reader in ["history", "on_hand", "ship", "receive", "refresh_low_stock"] {
        assert!(
            carrier.non_reader_methods.contains(non_reader),
            "`{non_reader}` reads no written field"
        );
    }
}

#[test]
fn whole_object_equality_confirms_only_when_it_can_hold_the_written_field() {
    let index = index(&[(LIB, LEDGER), ("tests/ledger.rs", LEDGER_TESTS)]);
    let test = the_test(&index);
    let carrier = establish(&index, "self.refresh_low_stock(sku);");
    assert!(
        carrier.is_some(),
        "the ledger shape must establish a carrier"
    );
    let Some(carrier) = carrier else { return };
    let cases = [
        // Cannot hold `low_stock`: a non-reading getter, a value returned by
        // a non-reading method, std methods on those values.
        (
            r#"assert_eq!(inv.history(), &[Event::Received { sku: "BOLT-M8".to_string(), qty: 10 }]);"#,
            false,
        ),
        (
            r#"assert_eq!(receipt, Receipt { sku: "BOLT-M8".to_string(), qty: 3, remaining: 7 });"#,
            false,
        ),
        ("assert_eq!(inv.history().to_vec(), Vec::new());", false),
        // Reads `low_stock` through a reader method or holds the receiver.
        (
            r#"assert_eq!(inv.low_skus(), vec!["BOLT-M8".to_string()]);"#,
            true,
        ),
        (
            r#"assert_eq!(vec![inv.is_low("BOLT-M8")], vec![true]);"#,
            true,
        ),
        ("assert_eq!(inv, fresh);", true),
        ("assert_eq!(copy, fresh);", true),
        ("assert_eq!(low, Vec::<String>::new());", true),
        (r#"assert_eq!(format!("{:?}", inv), String::new());"#, true),
        ("assert_eq!(inv.low_stock, BTreeSet::new());", true),
        // A binding with no `let` in the test is unknown: it may carry.
        (
            r#"assert_eq!(unbound, Receipt { sku: "X".to_string(), qty: 1, remaining: 0 });"#,
            true,
        ),
    ];
    for (text, expected) in cases {
        assert_eq!(
            carrier.admits(test, &whole_object(text)),
            expected,
            "admission of `{text}`"
        );
    }
    // Mock expectations and snapshots keep the Part C reading.
    for kind in [OracleKind::MockExpectation, OracleKind::Snapshot] {
        let mut fact = whole_object("assert_eq!(inv.history(), Vec::new());");
        fact.kind = kind;
        assert!(carrier.admits(test, &fact));
    }
}

#[test]
fn unbounded_effects_keep_the_part_c_reading() {
    let base = index(&[(LIB, LEDGER)]);
    // Not a `self.callee(..)` call, or a `?` / `&mut` argument.
    for expression in [
        "self.log.push(Event::Received { sku: sku.to_string(), qty })",
        "self.refresh_low_stock(sku)?",
        "self.refresh_low_stock(&mut buffer)",
        "refresh_low_stock(sku)",
        "self.missing(sku)",
    ] {
        assert!(
            establish(&base, expression).is_none(),
            "`{expression}` must not establish a carrier"
        );
    }

    let variant = |from: &str, to: &str| {
        assert!(LEDGER.contains(from), "fixture must contain `{from}`");
        LEDGER.replace(from, to)
    };
    let unbounded = [
        // A free function call may reach global state.
        variant(
            "self.low_stock.remove(sku);",
            "self.low_stock.remove(sku);\n            notify(sku);",
        ),
        // A non-pure macro performs I/O.
        variant(
            "self.low_stock.remove(sku);",
            "self.low_stock.remove(sku);\n            println!(\"low\");",
        ),
        // The whole receiver escapes.
        variant(
            "self.low_stock.remove(sku);",
            "self.low_stock.remove(sku);\n            audit(self);",
        ),
        // Interior mutability is invisible to a field scan.
        variant(
            "self.low_stock.remove(sku);",
            "self.low_stock.remove(sku);\n            self.hits.borrow_mut().push(1);",
        ),
        // `&self` callee: any write is interior mutability.
        variant(
            "fn refresh_low_stock(&mut self, sku: &str)",
            "fn refresh_low_stock(&self, sku: &str)",
        ),
        // A returned value can flow through the owner.
        variant(
            "fn refresh_low_stock(&mut self, sku: &str) {",
            "fn refresh_low_stock(&mut self, sku: &str) -> bool {",
        ),
        // A transitive self call that does not resolve.
        variant("if self.on_hand(sku)", "if self.level(sku)"),
        // A trait also names the callee: the call may not reach the impl.
        variant(
            "impl Inventory {",
            "pub trait Refresh { fn refresh_low_stock(&mut self, sku: &str); }\n\nimpl Inventory {",
        ),
    ];
    for source in &unbounded {
        let index = index(&[(LIB, source)]);
        assert!(
            establish(&index, "self.refresh_low_stock(sku)").is_none(),
            "an unbounded callee must keep the Part C reading:\n{source}"
        );
    }
}
