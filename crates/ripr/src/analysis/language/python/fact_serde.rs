//! Serde adapters for Python fact fields whose types do not serialize
//! themselves: interned `&'static str` labels and parser text ranges.

use rustpython_parser::text_size::{TextRange, TextSize};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// [`super::term_activation::PythonGuardedReturn`] with its comparison
/// operator owned, interned back to the static spelling on load.
#[derive(Deserialize)]
pub(super) struct GuardedReturnWire {
    parameter: String,
    operator: String,
    operand: String,
    then_value: String,
    else_value: String,
}

impl TryFrom<GuardedReturnWire> for super::term_activation::PythonGuardedReturn {
    type Error = String;

    fn try_from(wire: GuardedReturnWire) -> Result<Self, String> {
        let operator = ["<", "<=", ">", ">=", "==", "!="]
            .into_iter()
            .find(|known| *known == wire.operator)
            .ok_or_else(|| format!("unknown comparison `{}`; report this as a ripr bug with the fact file that carried it", wire.operator))?;
        Ok(Self {
            parameter: wire.parameter,
            operator,
            operand: wire.operand,
            then_value: wire.then_value,
            else_value: wire.else_value,
        })
    }
}

pub(super) fn python_language() -> &'static str {
    crate::domain::LanguageId::Python.as_str()
}

fn pair(range: &TextRange) -> (u32, u32) {
    (range.start().into(), range.end().into())
}

fn range((start, end): (u32, u32)) -> TextRange {
    TextRange::new(TextSize::from(start), TextSize::from(end.max(start)))
}

pub(super) mod option_range {
    use super::*;
    pub(in crate::analysis::language::python) fn serialize<S: Serializer>(
        value: &Option<TextRange>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.as_ref().map(pair).serialize(serializer)
    }
    pub(in crate::analysis::language::python) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<TextRange>, D::Error> {
        Ok(Option::<(u32, u32)>::deserialize(deserializer)?.map(range))
    }
}

pub(super) mod named_ranges {
    use super::*;
    pub(in crate::analysis::language::python) fn serialize<S: Serializer>(
        value: &[(String, TextRange)],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value
            .iter()
            .map(|(name, text_range)| (name, pair(text_range)))
            .collect::<Vec<_>>()
            .serialize(serializer)
    }
    pub(in crate::analysis::language::python) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<(String, TextRange)>, D::Error> {
        Ok(Vec::<(String, (u32, u32))>::deserialize(deserializer)?
            .into_iter()
            .map(|(name, text_range)| (name, range(text_range)))
            .collect())
    }
}

impl<'de> Deserialize<'de> for super::term_activation::PythonGuardedReturn {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(GuardedReturnWire::deserialize(deserializer)?)
            .map_err(serde::de::Error::custom)
    }
}
