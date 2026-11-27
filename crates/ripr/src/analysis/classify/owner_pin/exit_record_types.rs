//! The type names of the records an owner can return, read once per owner
//! text in a run scope (#11738).
//!
//! [`super::OwnerReturnPin::returned_types_drop_plain`] asks, for every pin
//! it establishes, whether each record type built in an exit position of the
//! owner has at most a plain `Drop` impl. The exit positions are a pure
//! function of the owner's text, but they were found by a fresh parse and a
//! walk of the whole body for every changed line and every repo seam the
//! owner holds, so one long owner with many changed lines paid its length
//! once per line. The text half is read here once per scope; the workspace
//! half (`drop_is_plain`) is still asked per name, in the same order.

use crate::analysis::syntax::parse_clean_source_file;
use crate::analysis::syntax::run_memo::{self, Product};
use ra_ap_syntax::{AstNode, ast};
use std::rc::Rc;

/// For each record built in an exit position of the owner `source`, in the
/// order [`super::exit_position_records`] finds them, the last path segment
/// that names its type, or `None` where the record has no readable path.
/// `None` overall when the text does not parse cleanly into a function with
/// a body, which the caller reads as "not plain".
pub(super) fn exit_record_types(source: &str) -> Rc<Option<Vec<Option<String>>>> {
    run_memo::memoize(Product::ExitRecordTypes, 0, source, || {
        Rc::new(read(source))
    })
}

fn read(source: &str) -> Option<Vec<Option<String>>> {
    #[cfg(test)]
    EXIT_RECORD_READS.with(|reads| reads.set(reads.get() + 1));
    let parse = parse_clean_source_file(source)?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    let body = function.body()?;
    Some(
        super::exit_position_records(&body)
            .iter()
            .map(|record| {
                record
                    .path()
                    .and_then(|path| path.segment())
                    .map(|segment| segment.syntax().text().to_string())
            })
            .collect(),
    )
}

#[cfg(test)]
thread_local! {
    /// How many owner texts this thread has read for exit record types.
    static EXIT_RECORD_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod tests;
