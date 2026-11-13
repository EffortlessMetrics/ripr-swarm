use super::*;
use std::fs::File;
use std::io::Read;
use std::path::Path;

struct UniqueJson(serde_json::Value);

impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = UniqueJson;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("JSON without duplicate object keys")
            }

            fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(UniqueJson(value.into()))
            }

            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(UniqueJson(value.into()))
            }

            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(UniqueJson(value.into()))
            }

            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
                serde_json::Number::from_f64(value)
                    .map(|number| UniqueJson(number.into()))
                    .ok_or_else(|| E::custom("non-finite JSON number"))
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(UniqueJson(value.into()))
            }

            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(UniqueJson(value.into()))
            }

            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueJson(serde_json::Value::Null))
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(UniqueJson(value)) = sequence.next_element()? {
                    values.push(value);
                }
                Ok(UniqueJson(values.into()))
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some((key, UniqueJson(value))) = map.next_entry::<String, UniqueJson>()? {
                    if values.insert(key.clone(), value).is_some() {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate JSON key: {key}"
                        )));
                    }
                }
                Ok(UniqueJson(values.into()))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

pub(super) fn parse_input(bytes: &[u8]) -> PlanResult<SelectionInput> {
    let UniqueJson(value) = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    serde_json::from_value(value).map_err(|error| error.to_string())
}

fn read_input(path: &Path) -> PlanResult<Vec<u8>> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("inspect {}: {error}", path.display()))?;
    require(metadata.is_file(), "input must be a regular file")?;
    let file = File::open(path)
        .map_err(|error| format!("open {}: {error}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    require(
        u64::try_from(bytes.len()).map_err(|error| error.to_string())? <= MAX_INPUT_BYTES,
        "input exceeds 4 MiB",
    )?;
    Ok(bytes)
}

fn normalize_input(input: &mut SelectionInput) {
    input.claim_index.global_authority_paths.sort();
    for claim in &mut input.claim_index.claims {
        claim.owners.sort();
        claim.source_closure.sort();
        claim.authority_paths.sort();
        claim.subjects.sort();
        claim.limitations.sort();
    }
    for changed in &mut input.change_set.changes {
        changed.semantic_subjects.sort();
    }
    input
        .change_set
        .changes
        .sort_by(|left, right| left.path.cmp(&right.path));
    input.change_set.limitations.sort();
}

fn execute(args: &[String]) -> PlanResult<String> {
    let mut input_path = None;
    let mut format = OutputFormat::Json;
    let mut iterator = args.iter();
    while let Some(flag) = iterator.next() {
        let value = iterator
            .next()
            .ok_or_else(|| format!("{flag} requires a value; {USAGE}"))?;
        match flag.as_str() {
            "--input" => {
                require(input_path.is_none(), "--input may be supplied only once")?;
                input_path = Some(value);
            }
            "--format" => {
                format = match value.as_str() {
                    "json" => OutputFormat::Json,
                    "markdown" => OutputFormat::Markdown,
                    _ => return Err(format!("unknown format {value}; {USAGE}")),
                };
            }
            _ => return Err(format!("unknown argument {flag}; {USAGE}")),
        }
    }
    let path = input_path.ok_or_else(|| format!("--input is required; {USAGE}"))?;
    let bytes = read_input(Path::new(path))?;
    let mut input = parse_input(&bytes)?;
    normalize_input(&mut input);
    let plan = plan(&input, sha256(&bytes))?;
    match format {
        OutputFormat::Json => {
            serde_json::to_string_pretty(&plan).map_err(|error| error.to_string())
        },
        OutputFormat::Markdown => Ok(render_markdown(&plan)),
    }
}

pub(super) fn entry() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if matches!(args.as_slice(), [flag] if flag == "--help" || flag == "-h") {
        println!("{USAGE}");
        return std::process::ExitCode::SUCCESS;
    }
    match execute(&args) {
        Ok(output) => {
            println!("{output}");
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("formal-claim-plan: {error}");
            std::process::ExitCode::from(2)
        }
    }
}
