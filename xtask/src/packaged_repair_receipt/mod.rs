mod gate;
mod model;
mod parse;
mod render;

#[cfg(test)]
mod tests;

pub(crate) use gate::evaluate;
pub(crate) use model::{ExpectedFixtureIdentity, GateInput, GateVerdict};
pub(crate) use parse::parse_receipt;
pub(crate) use render::{render_gate_json, render_gate_markdown, render_receipt_json};
