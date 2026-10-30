//! The value semantics the generated workflow's summary step used to get
//! from `jq -r ... 2>/dev/null || echo FALLBACK` and its shell helpers.
//!
//! The step summary read each artifact field through a jq query whose
//! failure printed a fallback word. Readers of existing summaries learned
//! those words (`unknown`, `0`, `none`), so this module keeps them: a
//! missing or malformed artifact, or a field whose type breaks the query,
//! still renders the same fallback rather than a new error shape. Only the
//! jq behaviours those queries depend on are modelled here.

use serde_json::{Map, Value};
use std::cmp::Ordering;
use std::fs;
use std::path::Path;

/// A query that failed the way a jq type error does; the caller renders its
/// fallback word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Fail;

pub(super) type Q = Result<Value, Fail>;

/// One artifact as jq read it: the JSON values it parsed, in order, and
/// whether it stopped on unreadable or malformed input.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Doc {
    pub(super) values: Vec<Value>,
    pub(super) broken: bool,
}

impl Doc {
    pub(super) fn read(path: &Path) -> Self {
        match fs::read(path) {
            Ok(bytes) => Self::parse(&bytes),
            Err(_) => Self {
                values: Vec::new(),
                broken: true,
            },
        }
    }

    pub(super) fn parse(bytes: &[u8]) -> Self {
        let mut values = Vec::new();
        let mut broken = false;
        for value in serde_json::Deserializer::from_slice(bytes).into_iter::<Value>() {
            match value {
                Ok(value) => values.push(value),
                Err(_) => {
                    broken = true;
                    break;
                }
            }
        }
        Self { values, broken }
    }

    /// `$(jq -r QUERY file 2>/dev/null || echo FAIL)`, captured. jq runs
    /// the query on every value and exits nonzero when the last one failed
    /// or the input was malformed; then `FAIL` follows whatever it printed.
    /// An empty `fail` is `|| true`, which prints nothing.
    pub(super) fn text(&self, query: impl Fn(&Value) -> Q, fail: &str) -> String {
        let mut printed = String::new();
        let mut failed = false;
        for value in &self.values {
            match query(value) {
                Ok(result) => {
                    printed.push_str(&raw(&result));
                    printed.push('\n');
                    failed = false;
                }
                Err(Fail) => failed = true,
            }
        }
        if (failed || self.broken) && !fail.is_empty() {
            printed.push_str(fail);
        }
        captured(&printed)
    }

    /// `.a.b // .c // "default"`, failing to `fail`.
    pub(super) fn field(&self, paths: &[&str], default: &str, fail: &str) -> String {
        self.text(|value| alt(value, paths, Value::from(default)), fail)
    }

    /// `.a.b // empty`: empty when every path is null or false.
    pub(super) fn optional(&self, paths: &[&str]) -> String {
        self.text(|value| alt(value, paths, Value::from("")), "")
    }

    /// `(.path // [] | length)`, failing to `0`.
    pub(super) fn length(&self, path: &str) -> String {
        self.text(
            |value| length(&alt(value, &[path], Value::Array(Vec::new()))?),
            "0",
        )
    }
}

/// Bash command substitution drops every trailing newline.
pub(super) fn captured(text: &str) -> String {
    text.trim_end_matches('\n').to_string()
}

/// The workflow's `markdown_inline`: one line, with backticks escaped so
/// the value cannot close the surrounding code span.
pub(super) fn inline(text: &str) -> String {
    text.replace(['\r', '\n'], " ").replace('`', "\\`")
}

/// `jq -r` output for one value.
pub(super) fn raw(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(_) | Value::Object(_) => {
            serde_json::to_string_pretty(value).unwrap_or_default()
        }
        other => other.to_string(),
    }
}

/// String interpolation and `tostring`: strings raw, anything else as
/// compact JSON.
pub(super) fn interpolated(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// `.key` on one value: null passes through, a non-object is a type error.
pub(super) fn index<'a>(value: &'a Value, key: &str) -> Result<&'a Value, Fail> {
    match value {
        Value::Null => Ok(&Value::Null),
        Value::Object(map) => Ok(map.get(key).unwrap_or(&Value::Null)),
        _ => Err(Fail),
    }
}

/// `.a.b.c` for a dotted path.
pub(super) fn at<'a>(value: &'a Value, path: &str) -> Result<&'a Value, Fail> {
    path.split('.').try_fold(value, index)
}

/// jq truthiness: everything except `null` and `false`.
pub(super) fn truthy(value: &Value) -> bool {
    !matches!(value, Value::Null | Value::Bool(false))
}

/// `.p1 // .p2 // default`. An error on the left of `//` is not
/// suppressed in jq, so it fails the whole query.
pub(super) fn alt(value: &Value, paths: &[&str], default: Value) -> Q {
    for path in paths {
        let found = at(value, path)?;
        if truthy(found) {
            return Ok(found.clone());
        }
    }
    Ok(default)
}

/// jq `length`.
pub(super) fn length(value: &Value) -> Q {
    match value {
        Value::Null => Ok(Value::from(0)),
        Value::Bool(_) => Err(Fail),
        Value::Number(number) => Ok(number
            .as_i64()
            .map(|n| Value::from(n.unsigned_abs()))
            .or_else(|| number.as_u64().map(Value::from))
            .or_else(|| number.as_f64().map(|n| Value::from(n.abs())))
            .unwrap_or(Value::Null)),
        Value::String(text) => Ok(Value::from(text.chars().count())),
        Value::Array(items) => Ok(Value::from(items.len())),
        Value::Object(map) => Ok(Value::from(map.len())),
    }
}

/// jq `+`: null is the identity, strings concatenate, numbers add.
pub(super) fn add(left: Value, right: Value) -> Q {
    match (left, right) {
        (Value::Null, other) | (other, Value::Null) => Ok(other),
        (Value::String(mut left), Value::String(right)) => {
            left.push_str(&right);
            Ok(Value::String(left))
        }
        (Value::Number(left), Value::Number(right)) => Ok(left
            .as_i64()
            .zip(right.as_i64())
            .and_then(|(left, right)| left.checked_add(right))
            .map(Value::from)
            .unwrap_or_else(|| {
                Value::from(left.as_f64().unwrap_or_default() + right.as_f64().unwrap_or_default())
            })),
        (Value::Array(mut left), Value::Array(right)) => {
            left.extend(right);
            Ok(Value::Array(left))
        }
        (Value::Object(mut left), Value::Object(right)) => {
            left.extend(right);
            Ok(Value::Object(left))
        }
        _ => Err(Fail),
    }
}

/// `.[]?`: an array's items or an object's values; nothing otherwise.
pub(super) fn each(value: &Value) -> Vec<&Value> {
    match value {
        Value::Array(items) => items.iter().collect(),
        Value::Object(map) => map.values().collect(),
        _ => Vec::new(),
    }
}

/// `.key[]?`: the `?` covers the iteration, not the index.
pub(super) fn each_at<'a>(value: &'a Value, key: &str) -> Result<Vec<&'a Value>, Fail> {
    index(value, key).map(each)
}

/// `"(.path // "unknown") + (if .line then ":" + (.line|tostring) else "" end)"`,
/// the location shape several at-a-glance lines share.
pub(super) fn location(value: &Value, path_key: &str, unknown: &str) -> Q {
    let path = alt(value, &[path_key], Value::from(unknown))?;
    let line = index(value, "line")?;
    let suffix = if truthy(line) {
        Value::from(format!(":{}", interpolated(line)))
    } else {
        Value::from("")
    };
    add(path, suffix)
}

/// jq `join(sep)` over an array.
pub(super) fn join(items: &[Value], separator: &str) -> Result<String, Fail> {
    let mut joined = String::new();
    for (position, item) in items.iter().enumerate() {
        if position > 0 {
            joined.push_str(separator);
        }
        match item {
            Value::Null => {}
            Value::String(text) => joined.push_str(text),
            Value::Bool(_) | Value::Number(_) => joined.push_str(&item.to_string()),
            Value::Array(_) | Value::Object(_) => return Err(Fail),
        }
    }
    Ok(joined)
}

/// `if length == 0 then "none" else join(", ") end`.
pub(super) fn join_or_none(items: &[Value]) -> Q {
    if items.is_empty() {
        Ok(Value::from("none"))
    } else {
        join(items, ", ").map(Value::from)
    }
}

/// jq `sort`/`unique` ordering: null < false < true < numbers < strings <
/// arrays < objects.
pub(super) fn compare(left: &Value, right: &Value) -> Ordering {
    fn rank(value: &Value) -> u8 {
        match value {
            Value::Null => 0,
            Value::Bool(false) => 1,
            Value::Bool(true) => 2,
            Value::Number(_) => 3,
            Value::String(_) => 4,
            Value::Array(_) => 5,
            Value::Object(_) => 6,
        }
    }
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => left
            .as_f64()
            .unwrap_or_default()
            .total_cmp(&right.as_f64().unwrap_or_default()),
        (Value::String(left), Value::String(right)) => left.cmp(right),
        (Value::Array(left), Value::Array(right)) => left
            .iter()
            .zip(right)
            .map(|(left, right)| compare(left, right))
            .find(|ordering| ordering.is_ne())
            .unwrap_or_else(|| left.len().cmp(&right.len())),
        (Value::Object(_), Value::Object(_)) => left.to_string().cmp(&right.to_string()),
        _ => rank(left).cmp(&rank(right)),
    }
}

/// `sort | group_by(.) | map("\(.[0])=\(length)") | if length == 0 then
/// "none" else join(", ") end`.
pub(super) fn tally(mut values: Vec<Value>) -> Value {
    values.sort_by(compare);
    let mut groups: Vec<(Value, usize)> = Vec::new();
    for value in values {
        match groups.last_mut() {
            Some((last, count)) if compare(last, &value).is_eq() => *count += 1,
            _ => groups.push((value, 1)),
        }
    }
    if groups.is_empty() {
        return Value::from("none");
    }
    Value::from(
        groups
            .iter()
            .map(|(value, count)| format!("{}={count}", interpolated(value)))
            .collect::<Vec<_>>()
            .join(", "),
    )
}

/// `unique`.
pub(super) fn unique(mut values: Vec<Value>) -> Vec<Value> {
    values.sort_by(compare);
    values.dedup_by(|right, left| compare(left, right).is_eq());
    values
}

/// `.. | objects`, in document order.
pub(super) fn objects<'a>(value: &'a Value, found: &mut Vec<&'a Map<String, Value>>) {
    match value {
        Value::Object(map) => {
            found.push(map);
            for child in map.values() {
                objects(child, found);
            }
        }
        Value::Array(items) => {
            for child in items {
                objects(child, found);
            }
        }
        _ => {}
    }
}

/// The workflow's `repo_relative`: the checkout path, where it is a whole
/// path token, becomes the repository root `.`. The physical root is
/// rewritten first, then the logical one, line by line.
#[derive(Debug, Clone, Default)]
pub(super) struct RepoRelative {
    pub(super) physical: Vec<u8>,
    pub(super) logical: Vec<u8>,
}

impl RepoRelative {
    /// `printf '%s\n' "$value" | repo_relative`, captured.
    pub(super) fn value(&self, text: &str) -> String {
        let lines = text
            .split('\n')
            .map(|line| String::from_utf8_lossy(&self.line(line.as_bytes())).into_owned())
            .collect::<Vec<_>>();
        captured(&lines.join("\n"))
    }

    /// `repo_relative < file`: every input line, newline-terminated.
    pub(super) fn file(&self, bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(bytes.len());
        let body = bytes.strip_suffix(b"\n").unwrap_or(bytes);
        if bytes.is_empty() {
            return out;
        }
        for line in body.split(|byte| *byte == b'\n') {
            out.extend(self.line(line));
            out.push(b'\n');
        }
        out
    }

    fn line(&self, line: &[u8]) -> Vec<u8> {
        rewrite(&rewrite(line, &self.physical), &self.logical)
    }
}

fn rewrite(text: &[u8], root: &[u8]) -> Vec<u8> {
    if root.is_empty() {
        return text.to_vec();
    }
    let mut out = Vec::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = find(rest, root) {
        let (pre, tail) = rest.split_at(start);
        let after_root = tail.get(root.len()..).unwrap_or_default();
        let before_ok = pre.last().is_none_or(|byte| b" '\"`=(".contains(byte));
        let after_ok = after_root
            .first()
            .is_none_or(|byte| b"/ '\"`):".contains(byte));
        out.extend_from_slice(pre);
        if before_ok && after_ok {
            out.push(b'.');
        } else {
            out.extend_from_slice(root);
        }
        rest = after_root;
    }
    out.extend_from_slice(rest);
    out
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parsed(text: &str) -> Doc {
        let doc = Doc::parse(text.as_bytes());
        assert!(!doc.broken, "fixture must parse: {text}");
        doc
    }

    #[test]
    fn an_unreadable_file_is_broken_and_a_readable_one_parses()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(format!("ripr-jq-read-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let present = dir.join("present.json");
        std::fs::write(&present, "{\"a\":\"x\"} 2")?;
        let read = Doc::read(&present);
        assert_eq!(read.values, vec![json!({"a": "x"}), json!(2)]);
        assert!(!read.broken);
        // A missing file and a directory both read as broken and empty, so a
        // field falls back to its word instead of an error shape.
        let missing = Doc::read(&dir.join("missing.json"));
        let directory = Doc::read(&dir);
        std::fs::remove_dir_all(&dir)?;
        for unreadable in [&missing, &directory] {
            assert!(unreadable.values.is_empty());
            assert!(unreadable.broken);
            assert_eq!(unreadable.field(&["a"], "d", "unknown"), "unknown");
            assert_eq!(unreadable.length("a"), "0");
            assert_eq!(unreadable.optional(&["a"]), "");
        }
        Ok(())
    }

    #[test]
    fn doc_field_optional_and_length_follow_the_retired_queries() {
        let doc = parsed("{\"a\":{\"b\":\"deep\"},\"c\":null,\"d\":[1,2,3],\"e\":false}");
        assert_eq!(doc.field(&["a.b"], "dflt", "unknown"), "deep");
        assert_eq!(doc.field(&["c", "a.b"], "dflt", "unknown"), "deep");
        assert_eq!(doc.field(&["c", "e"], "dflt", "unknown"), "dflt");
        // A path through a scalar is a type error and takes the fail word.
        assert_eq!(doc.field(&["d.x"], "dflt", "unknown"), "unknown");
        assert_eq!(doc.optional(&["c", "e"]), "");
        assert_eq!(doc.optional(&["a.b"]), "deep");
        assert_eq!(doc.length("d"), "3");
        assert_eq!(doc.length("c"), "0");
        assert_eq!(doc.length("missing"), "0");
        // `false` has no length: the query fails and prints `0` anyway.
        assert_eq!(doc.length("e"), "0");
        assert_eq!(doc.length("a"), "1");
        // An empty fail word prints nothing, as `|| true` did.
        assert_eq!(doc.text(|value| at(value, "d.x").cloned(), ""), "");
    }

    #[test]
    fn raw_and_interpolated_print_the_way_jq_does() {
        assert_eq!(raw(&json!("s")), "s");
        assert_eq!(raw(&json!(7)), "7");
        assert_eq!(raw(&json!(null)), "null");
        assert_eq!(raw(&json!(true)), "true");
        assert_eq!(raw(&json!([1])), "[\n  1\n]");
        assert_eq!(raw(&json!({"k": "v"})), "{\n  \"k\": \"v\"\n}");
        assert_eq!(interpolated(&json!("s")), "s");
        assert_eq!(interpolated(&json!([1, "a"])), "[1,\"a\"]");
        assert_eq!(interpolated(&json!(null)), "null");
        assert_eq!(inline("a`b\r\nc"), "a\\`b  c");
        assert_eq!(captured("x\n\n"), "x");
        assert_eq!(captured("\nx"), "\nx");
    }

    #[test]
    fn indexing_distinguishes_null_pass_through_from_type_errors() {
        let object = json!({"a": {"b": 1}, "n": null});
        assert_eq!(index(&Value::Null, "k"), Ok(&Value::Null));
        assert_eq!(index(&object, "missing"), Ok(&Value::Null));
        assert_eq!(index(&json!("s"), "k"), Err(Fail));
        assert_eq!(index(&json!([1]), "k"), Err(Fail));
        assert_eq!(at(&object, "a.b"), Ok(&json!(1)));
        assert_eq!(at(&object, "n.x.y"), Ok(&Value::Null));
        assert_eq!(at(&object, "a.b.c"), Err(Fail));
        assert!(!truthy(&Value::Null));
        assert!(!truthy(&json!(false)));
        assert!(truthy(&json!(0)));
        assert!(truthy(&json!("")));
        assert!(truthy(&json!([])));
        // An error on the left of `//` is not suppressed.
        assert_eq!(alt(&object, &["a.b.c", "n"], json!("d")), Err(Fail));
        assert_eq!(alt(&object, &["n", "a.b"], json!("d")), Ok(json!(1)));
        assert_eq!(alt(&object, &["n"], json!("d")), Ok(json!("d")));
    }

    #[test]
    fn length_follows_jq_for_every_value_kind() {
        assert_eq!(length(&Value::Null), Ok(json!(0)));
        assert_eq!(length(&json!(true)), Err(Fail));
        assert_eq!(length(&json!(false)), Err(Fail));
        assert_eq!(length(&json!(-5)), Ok(json!(5)));
        assert_eq!(length(&json!(u64::MAX)), Ok(json!(u64::MAX)));
        assert_eq!(length(&json!(-2.5)), Ok(json!(2.5)));
        assert_eq!(length(&json!("héé")), Ok(json!(3)));
        assert_eq!(length(&json!([1, 2])), Ok(json!(2)));
        assert_eq!(length(&json!({"a": 1})), Ok(json!(1)));
    }

    #[test]
    fn add_follows_jq_for_every_pairing() {
        assert_eq!(add(Value::Null, json!("x")), Ok(json!("x")));
        assert_eq!(add(json!(3), Value::Null), Ok(json!(3)));
        assert_eq!(add(Value::Null, Value::Null), Ok(Value::Null));
        assert_eq!(add(json!("a"), json!("b")), Ok(json!("ab")));
        assert_eq!(add(json!(1), json!(2)), Ok(json!(3)));
        assert_eq!(add(json!(1.5), json!(2)), Ok(json!(3.5)));
        // Integer overflow falls back to floating point instead of wrapping.
        assert_eq!(
            add(json!(i64::MAX), json!(1)),
            Ok(json!(i64::MAX as f64 + 1.0))
        );
        assert_eq!(add(json!([1]), json!([2])), Ok(json!([1, 2])));
        assert_eq!(
            add(json!({"a": 1, "b": 1}), json!({"b": 2})),
            Ok(json!({"a": 1, "b": 2}))
        );
        for (left, right) in [
            (json!("a"), json!(1)),
            (json!(1), json!("a")),
            (json!([1]), json!({"a": 1})),
            (json!(true), json!(true)),
            (json!(true), json!("a")),
        ] {
            assert_eq!(
                add(left.clone(), right.clone()),
                Err(Fail),
                "{left} + {right}"
            );
        }
    }

    #[test]
    fn iteration_takes_array_items_and_object_values_only() {
        let doc = json!({"list": [1, 2], "map": {"a": 3, "b": 4}, "scalar": 5});
        assert_eq!(each_at(&doc, "list"), Ok(vec![&json!(1), &json!(2)]));
        assert_eq!(each_at(&doc, "map"), Ok(vec![&json!(3), &json!(4)]));
        assert_eq!(each_at(&doc, "scalar"), Ok(Vec::new()));
        assert_eq!(each_at(&doc, "missing"), Ok(Vec::new()));
        // The `?` covers the iteration, not the index.
        assert_eq!(each_at(&json!("s"), "list"), Err(Fail));
        assert!(each(&json!("s")).is_empty());
    }

    #[test]
    fn location_names_the_path_and_the_line_only_when_present() {
        let at_line = json!({"path": "src/a.rs", "line": 4});
        assert_eq!(
            location(&at_line, "path", "unknown"),
            Ok(json!("src/a.rs:4"))
        );
        let no_line = json!({"path": "src/a.rs"});
        assert_eq!(location(&no_line, "path", "unknown"), Ok(json!("src/a.rs")));
        let false_line = json!({"path": "src/a.rs", "line": false});
        assert_eq!(
            location(&false_line, "path", "unknown"),
            Ok(json!("src/a.rs"))
        );
        let zero_line = json!({"line": 0});
        assert_eq!(
            location(&zero_line, "path", "unknown"),
            Ok(json!("unknown:0"))
        );
        assert_eq!(
            location(&json!({}), "path", "unknown"),
            Ok(json!("unknown"))
        );
        assert_eq!(location(&json!("s"), "path", "unknown"), Err(Fail));
    }

    #[test]
    fn join_prints_scalars_and_fails_on_containers() {
        let items = [json!("a"), Value::Null, json!(2), json!(true), json!("z")];
        assert_eq!(join(&items, ","), Ok("a,,2,true,z".to_string()));
        assert_eq!(join(&[], ","), Ok(String::new()));
        assert_eq!(join(&[json!(1), json!([2])], ","), Err(Fail));
        assert_eq!(join(&[json!({"a": 1})], ","), Err(Fail));
        assert_eq!(join_or_none(&[]), Ok(json!("none")));
        assert_eq!(join_or_none(&[json!("a"), json!("b")]), Ok(json!("a, b")));
        assert_eq!(join_or_none(&[json!([1])]), Err(Fail));
    }

    #[test]
    fn compare_orders_by_jq_kind_rank_then_by_value() {
        use std::cmp::Ordering::{Equal, Greater, Less};
        let ranked = [
            Value::Null,
            json!(false),
            json!(true),
            json!(-1),
            json!(2.5),
            json!("a"),
            json!("b"),
            json!([]),
            json!([1]),
            json!([1, 2]),
            json!([2]),
            json!({"a": 1}),
        ];
        for pair in ranked.windows(2) {
            assert_eq!(
                compare(&pair[0], &pair[1]),
                Less,
                "{} < {}",
                pair[0],
                pair[1]
            );
            assert_eq!(compare(&pair[1], &pair[0]), Greater);
        }
        assert_eq!(compare(&json!(1), &json!(1.0)), Equal);
        assert_eq!(compare(&json!({"a": 1}), &json!({"a": 1})), Equal);
        assert_eq!(compare(&json!({"a": 1}), &json!({"a": 2})), Less);
    }

    #[test]
    fn tally_and_unique_group_equal_values_in_jq_order() {
        assert_eq!(tally(Vec::new()), json!("none"));
        assert_eq!(
            tally(vec![
                json!("b"),
                json!("a"),
                json!("b"),
                json!(1),
                json!("a"),
                json!("b")
            ]),
            json!("1=1, a=2, b=3")
        );
        assert_eq!(tally(vec![json!([1]), json!([1])]), json!("[1]=2"));
        assert_eq!(
            unique(vec![
                json!("b"),
                json!("a"),
                json!("b"),
                json!(2),
                Value::Null
            ]),
            vec![Value::Null, json!(2), json!("a"), json!("b")]
        );
        assert!(unique(Vec::new()).is_empty());
    }

    #[test]
    fn objects_are_collected_in_document_order_through_arrays_and_maps() {
        let doc = json!({"a": {"b": [{"c": 1}, 2, [{"d": 3}]]}, "e": "s"});
        let mut found = Vec::new();
        objects(&doc, &mut found);
        let keys = found
            .iter()
            .map(|map| map.keys().cloned().collect::<Vec<_>>().join("+"))
            .collect::<Vec<_>>();
        assert_eq!(keys, vec!["a+e", "b", "c", "d"]);
        let mut none = Vec::new();
        let scalars = json!([1, "x", null]);
        objects(&scalars, &mut none);
        assert!(none.is_empty());
    }

    #[test]
    fn repo_relative_leaves_text_alone_when_no_root_applies() {
        let none = RepoRelative::default();
        assert_eq!(none.value("cd /w/repo"), "cd /w/repo");
        assert_eq!(none.file(b"a\nb"), b"a\nb\n");
        let relative = RepoRelative {
            physical: b"/w/repo".to_vec(),
            logical: Vec::new(),
        };
        // Every whole-token position the workflow's sed accepted.
        assert_eq!(relative.value("'/w/repo'"), "'.'");
        assert_eq!(relative.value("\"/w/repo\""), "\".\"");
        assert_eq!(relative.value("`/w/repo`"), "`.`");
        assert_eq!(relative.value("(/w/repo):x"), "(.):x");
        assert_eq!(relative.value("/w/repo:12"), ".:12");
        // A neighbouring word character on either side is not a token edge.
        assert_eq!(relative.value("x/w/repo"), "x/w/repo");
        assert_eq!(relative.value("/w/repo-2"), "/w/repo-2");
        assert_eq!(relative.value("/w/repo/a /w/repo/b"), "./a ./b");
        // A rewrite never reads outside the line that holds it.
        assert_eq!(relative.file(b"/w/repo\n/w/repox\n"), b".\n/w/repox\n");
    }
}
