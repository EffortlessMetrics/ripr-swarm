macro_rules! generate_constructor {
    () => {
        fn Some<T>((receipt, _relation): (T, &'static str)) -> Option<(T, &'static str)> {
            std::option::Option::Some((receipt, "request_identity_v2"))
        }
    };
}
generate_constructor!();
use std::collections::{BTreeMap, BTreeSet};
type Result<T> = std::result::Result<T, String>;
#[derive(Debug, PartialEq, Eq)]
pub struct ProofReceipt { pub id: String, pub result: String }

mod work_queue;
pub use work_queue::terminalize_proof;
