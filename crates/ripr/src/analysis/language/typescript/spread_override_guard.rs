//! RIPR-SPEC-0246 rule 3: a change that only drops (or adds) one literal
//! field after a spread needs a tested input that does not already carry it.
//!
//! `return { ...cfg, ok: true }` becoming `return { ...cfg }` changes the
//! result only for an input whose `ok` is not `true`. When the test passes
//! `{ name: "bo", ok: true }`, both versions return an object equal to the
//! input, however exact the matcher is. So an `exposed` ladder result whose
//! change is one such field keeps its credit only when an observed owner
//! call passes an input that lacks the field or holds another literal there.

use super::{identifier_offsets, ts_local_only_read, ts_mask_literals_and_comments};

/// The field one side of the change sets after spreading `subject` and the
/// other side lacks: `return { ...x, k: L }` against `return { ...x }` (other
/// entries equal), in either direction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SpreadOverride {
    /// The spread identifier, which must be an owner parameter.
    pub(crate) subject: String,
    /// The overriding key.
    pub(crate) key: String,
    /// The overriding literal, as written.
    pub(crate) literal: String,
    /// The overriding literal's value.
    value: LiteralValue,
}

/// What one tested input shows about the overriding field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OverrideInput {
    /// The input holds the same literal under the key: both versions agree.
    Carries,
    /// The input lacks the key or holds another literal: the versions differ.
    Differs,
    /// The input cannot be read (a spread, a computed or accessor entry, a
    /// non-literal value, or no visible object literal).
    Unknown,
}

/// A number keeps its bits, so `-0` and `0` differ, as `toEqual` tells
/// them apart, while `1` and `1.0` match.
#[derive(Clone, Debug, PartialEq, Eq)]
enum LiteralValue {
    Number(u64),
    Text(String),
    Word(&'static str),
}

impl SpreadOverride {
    /// Read the change, or `None` when it is not exactly one literal field
    /// added or dropped after a leading spread of one identifier.
    pub(crate) fn read(old_line: &str, new_line: &str) -> Option<Self> {
        let old = returned_object_entries(old_line)?;
        let new = returned_object_entries(new_line)?;
        let (longer, shorter) = if old.len() == new.len() + 1 {
            (old, new)
        } else if new.len() == old.len() + 1 {
            (new, old)
        } else {
            return None;
        };
        let subject = shorter.first()?.strip_prefix("...")?.trim().to_string();
        if !is_identifier(&subject) || longer.first()? != shorter.first()? {
            return None;
        }
        // Exactly one entry of the longer side is missing from the shorter
        // side, and the rest match in order.
        let extra = (1..longer.len())
            .find(|&at| longer[..at] == shorter[..at] && longer[at + 1..] == shorter[at..])?;
        let (key, literal) = key_value(&longer[extra])?;
        let value = literal_value(&literal)?;
        // A second spread could carry the key, and a second entry with the
        // key would override it.
        let clean = |entries: &[String]| {
            entries[1..].iter().all(|entry| {
                !entry.starts_with("...") && key_value(entry).is_some_and(|(other, _)| other != key)
            })
        };
        let others: Vec<String> = longer
            .iter()
            .enumerate()
            .filter(|&(at, _)| at != extra)
            .map(|(_, entry)| entry.clone())
            .collect();
        if !clean(&others) {
            return None;
        }
        Some(Self {
            subject,
            key,
            literal,
            value,
        })
    }

    /// What the object literal `object` (`{ ... }`, as written in a test)
    /// shows about this field.
    pub(crate) fn input(&self, object: &str) -> OverrideInput {
        let Some(entries) = object_entries(object.trim()) else {
            return OverrideInput::Unknown;
        };
        let mut found = None;
        for entry in &entries {
            if entry.starts_with("...") {
                return OverrideInput::Unknown;
            }
            match key_value(entry) {
                Some((key, literal)) if key == self.key => found = Some(literal),
                Some(_) => {}
                // A shorthand of another name cannot hold the key; anything
                // else (a computed key, a method, an accessor) might.
                None if is_identifier(entry) && entry != &self.key => {}
                None => return OverrideInput::Unknown,
            }
        }
        match found {
            None => OverrideInput::Differs,
            Some(literal) => match literal_value(&literal) {
                Some(value) if value == self.value => OverrideInput::Carries,
                Some(_) => OverrideInput::Differs,
                None => OverrideInput::Unknown,
            },
        }
    }
}

/// The object literal a test binds once to `name` with `const name = { ... }`,
/// or `None` when `name` is bound another way, more than once, or not at all.
pub(crate) fn ts_test_local_object(body: &str, name: &str, callees: &[&str]) -> Option<String> {
    let mut found = None;
    for keyword in ["const", "let", "var"] {
        let mut rest = body;
        let mut offset = 0;
        while let Some(at) = rest.find(keyword) {
            let start = offset + at;
            let before_ok = body[..start]
                .chars()
                .next_back()
                .is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'));
            let after = &body[start + keyword.len()..];
            offset = start + keyword.len();
            rest = after;
            if !before_ok || !after.starts_with(char::is_whitespace) {
                continue;
            }
            let declared = after.trim_start();
            let Some(tail) = declared.strip_prefix(name) else {
                continue;
            };
            if tail
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
            {
                continue;
            }
            if keyword != "const" || found.is_some() {
                return None;
            }
            let initializer = skip_type_annotation(tail.trim_start())?
                .strip_prefix('=')?
                .trim_start();
            if !initializer.starts_with('{') {
                return None;
            }
            found = Some(balanced_object(initializer)?.to_string());
        }
    }
    // A member written later (`cfg.ok = false`), any use but a read or a
    // whole owner or matcher argument, or a callback parameter of the same
    // name, means the call may not see the bound literal.
    let masked = ts_mask_literals_and_comments(body);
    if !ts_local_only_read(&masked, name, callees) || bound_as_parameter(&masked, name) {
        return None;
    }
    found
}

/// The text after a `: Type` annotation (up to the top-level `=` that is no
/// `=>`, `==` or `>=`), or `text` itself when no annotation opens it.
fn skip_type_annotation(text: &str) -> Option<&str> {
    let Some(annotation) = text.strip_prefix(':') else {
        return Some(text);
    };
    let mut depth = 0usize;
    let mut previous = ' ';
    for (at, ch) in annotation.char_indices() {
        match ch {
            '<' | '(' | '[' | '{' => depth += 1,
            '>' if previous != '=' => depth = depth.saturating_sub(1),
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            '=' if depth == 0
                && !annotation[at + 1..].starts_with(['>', '='])
                && !matches!(previous, '=' | '!' | '<' | '>') =>
            {
                return Some(&annotation[at..]);
            }
            _ => {}
        }
        previous = ch;
    }
    None
}

/// `true` when `name` is a parameter of a function or arrow written in the
/// masked body: `name => ...`, `(a, name) => ...` or `function f(name)`.
fn bound_as_parameter(masked: &str, name: &str) -> bool {
    // `identifier_offsets` skips `...name`, which binds in `({ ...name }) =>`.
    let spreads = masked
        .match_indices("...")
        .map(|(at, _)| at + 3)
        .filter(|&at| {
            masked[at..].starts_with(name)
                && !masked[at + name.len()..]
                    .starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '$')
        });
    identifier_offsets(masked, name).into_iter().chain(spreads).any(|at| {
        let after = &masked[at + name.len()..];
        if after.trim_start().starts_with("=>") {
            return true;
        }
        let mut depth = 0usize;
        let Some(open) = masked[..at].char_indices().rev().find_map(|(index, c)| {
            match c {
                ')' | ']' | '}' => depth += 1,
                '(' if depth == 0 => return Some(index),
                '(' | '[' | '{' => depth = depth.saturating_sub(1),
                _ => {}
            }
            None
        }) else {
            return false;
        };
        let mut depth = 0usize;
        let Some(close) = masked[open..].char_indices().find_map(|(index, c)| {
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(open + index + 1);
                    }
                }
                _ => {}
            }
            None
        }) else {
            return false;
        };
        let follows = masked[close..].trim_start();
        let head = masked[..open].trim_end();
        let word = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '$';
        // `(name): T => ...` is an arrow; `owner(name) : x` ends a ternary.
        follows.starts_with("=>")
            || (follows.starts_with(':') && !head.ends_with(word))
            || head.ends_with("function")
            || head.trim_end_matches(word).trim_end().ends_with("function")
    })
}

/// The entries of `return { ... }` on one line, trimmed.
fn returned_object_entries(line: &str) -> Option<Vec<String>> {
    let rest = line.trim().strip_prefix("return")?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let object = rest.trim().trim_end_matches(';').trim_end();
    if balanced_object(object)? != object {
        return None;
    }
    object_entries(object)
}

/// The prefix of `text` that is one balanced `{ ... }`.
fn balanced_object(text: &str) -> Option<&str> {
    if !text.starts_with('{') {
        return None;
    }
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (at, ch) in text.char_indices() {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == open {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' | '`' => quote = Some(ch),
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return (ch == '}').then(|| &text[..=at]);
                }
            }
            _ => {}
        }
    }
    None
}

/// The top-level entries of a balanced `{ ... }`, trimmed, with a trailing
/// comma allowed. A comment inside the object refuses the read.
fn object_entries(object: &str) -> Option<Vec<String>> {
    let inner = object.strip_prefix('{')?.strip_suffix('}')?;
    let mut entries = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut slash = false;
    for ch in inner.chars() {
        // A comment outside a string could hide or fake entries.
        let after_slash = std::mem::replace(&mut slash, quote.is_none() && ch == '/');
        if after_slash && (ch == '/' || ch == '*') {
            return None;
        }
        if let Some(open) = quote {
            current.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == open {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' | '`' => {
                quote = Some(ch);
                current.push(ch);
            }
            '{' | '(' | '[' => {
                depth += 1;
                current.push(ch);
            }
            '}' | ')' | ']' => {
                depth = depth.checked_sub(1)?;
                current.push(ch);
            }
            ',' if depth == 0 => entries.push(std::mem::take(&mut current)),
            _ => current.push(ch),
        }
    }
    if quote.is_some() || depth != 0 {
        return None;
    }
    entries.push(current);
    let mut entries: Vec<String> = entries.into_iter().map(|e| e.trim().to_string()).collect();
    if entries.last().is_some_and(String::is_empty) {
        entries.pop();
    }
    if entries.iter().any(String::is_empty) {
        return None;
    }
    Some(entries)
}

/// `key: value` with a plain or quoted key, or `None` for anything else.
fn key_value(entry: &str) -> Option<(String, String)> {
    let (key, value) = entry.split_once(':')?;
    let key = key.trim();
    let key = if let Some(quoted) = quoted_text(key) {
        quoted
    } else if is_identifier(key) {
        key.to_string()
    } else {
        return None;
    };
    Some((key, value.trim().to_string()))
}

fn literal_value(text: &str) -> Option<LiteralValue> {
    if let Some(text) = quoted_text(text) {
        return Some(LiteralValue::Text(text));
    }
    for word in ["true", "false", "null"] {
        if text == word {
            return Some(LiteralValue::Word(word));
        }
    }
    let digits = text.strip_prefix('-').unwrap_or(text);
    if digits.chars().next().is_some_and(|c| c.is_ascii_digit())
        && digits.chars().all(|c| c.is_ascii_digit() || c == '.')
    {
        return text
            .parse::<f64>()
            .ok()
            .map(|value| LiteralValue::Number(value.to_bits()));
    }
    None
}

/// The text of a `"..."` or `'...'` literal with no escape or quote inside.
fn quoted_text(text: &str) -> Option<String> {
    let quote = text.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let inner = text.strip_prefix(quote)?.strip_suffix(quote)?;
    (!inner.contains(['\\', '"', '\''])).then(|| inner.to_string())
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

#[cfg(test)]
mod tests {
    use super::super::ts_members_written;
    use super::*;

    #[test]
    fn reads_one_literal_field_after_a_spread_in_either_direction() {
        let dropped =
            SpreadOverride::read("  return { ...cfg, ok: true };", "  return { ...cfg };");
        assert_eq!(
            dropped
                .as_ref()
                .map(|o| (o.subject.as_str(), o.key.as_str(), o.literal.as_str())),
            Some(("cfg", "ok", "true"))
        );
        let added =
            SpreadOverride::read("return {...cfg, a: 1}", "return {...cfg, ok: \"y\", a: 1}");
        assert_eq!(added.map(|o| o.key), Some("ok".to_string()));
        // A quoted key, and a `//` inside a string, still read.
        let quoted = SpreadOverride::read(
            r#"return { ...cfg, url: "http://x", "ok": true };"#,
            r#"return { ...cfg, url: "http://x" };"#,
        );
        assert_eq!(quoted.map(|o| o.key), Some("ok".to_string()));
        for (old, new) in [
            // a replaced value, not a dropped field
            (
                "return { ...cfg, ok: true };",
                "return { ...cfg, ok: false };",
            ),
            // a non-literal value
            ("return { ...cfg, ok: flag };", "return { ...cfg };"),
            // no leading spread, or a spread of a non-identifier
            ("return { ok: true, ...cfg };", "return { ...cfg };"),
            ("return { ...load(), ok: true };", "return { ...load() };"),
            // a second spread could carry the key
            (
                "return { ...cfg, ok: true, ...more };",
                "return { ...cfg, ...more };",
            ),
            // a duplicate key
            (
                "return { ...cfg, ok: true, ok: false };",
                "return { ...cfg, ok: false };",
            ),
            // two fields
            ("return { ...cfg, ok: true, a: 1 };", "return { ...cfg };"),
            // not a whole returned object
            ("return { ...cfg, ok: true }.ok;", "return { ...cfg }.ok;"),
            ("const r = { ...cfg, ok: true };", "const r = { ...cfg };"),
            // a comment hides entries
            (
                "return { ...cfg, ok: true /* , x: 1 */ };",
                "return { ...cfg };",
            ),
            // a comment on its own entry would otherwise read as one field
            (
                "return { ...cfg, ok: true, // x\n };",
                "return { ...cfg, };",
            ),
            // an escape or quote inside the string is not read
            (r#"return { ...cfg, ok: "a\"b" };"#, "return { ...cfg };"),
            ("return { ...cfg, ok: 'it\\'s' };", "return { ...cfg };"),
        ] {
            assert_eq!(SpreadOverride::read(old, new), None, "{old} -> {new}");
        }
    }

    #[test]
    fn an_input_that_already_carries_the_field_cannot_see_it() -> Result<(), String> {
        let change = SpreadOverride::read("return { ...cfg, ok: true };", "return { ...cfg };")
            .ok_or("expected a spread override")?;
        for (object, expected) in [
            (r#"{ name: "bo", ok: true }"#, OverrideInput::Carries),
            (r#"{ "ok": true, name }"#, OverrideInput::Carries),
            (r#"{ ok: false, ok: true }"#, OverrideInput::Carries),
            (r#"{ name: "bo", ok: false }"#, OverrideInput::Differs),
            (r#"{ name: "bo" }"#, OverrideInput::Differs),
            (r#"{ ok: true, ok: false }"#, OverrideInput::Differs),
            (r#"{ ok: flag }"#, OverrideInput::Unknown),
            (r#"{ ...base, name: "bo" }"#, OverrideInput::Unknown),
            (r#"{ ok }"#, OverrideInput::Unknown),
            (r#"{ ["o" + "k"]: false }"#, OverrideInput::Unknown),
            (r#"{ get ok() { return false; } }"#, OverrideInput::Unknown),
            (r#"{ ok: true /* real: false */ }"#, OverrideInput::Unknown),
            (r#"{ url: "http://x", ok: true }"#, OverrideInput::Carries),
            (r#"{ url: "http://x" }"#, OverrideInput::Differs),
            (r#"{ ok: "a\"b" }"#, OverrideInput::Unknown),
            ("cfg", OverrideInput::Unknown),
        ] {
            assert_eq!(change.input(object), expected, "{object}");
        }
        let number = SpreadOverride::read("return { ...o, n: 1 };", "return { ...o };")
            .ok_or("expected a spread override")?;
        assert_eq!(number.input("{ n: 1.0 }"), OverrideInput::Carries);
        assert_eq!(number.input("{ n: \"1\" }"), OverrideInput::Differs);
        let zero = SpreadOverride::read("return { ...o, n: 0 };", "return { ...o };")
            .ok_or("expected a spread override")?;
        assert_eq!(zero.input("{ n: -0 }"), OverrideInput::Differs);
        Ok(())
    }

    #[test]
    fn an_owner_that_writes_or_passes_on_its_parameter_is_seen() {
        let written = |text: &str, any_call: bool| {
            ts_members_written(&ts_mask_literals_and_comments(text), "cfg", any_call)
        };
        for text in [
            "cfg.ok = false;",
            "cfg?.inner.ok += 1;",
            "cfg[\"ok\"] = false;",
            "cfg.count++;",
            "++cfg.count;",
            "cfg!.ok ??= true;",
            "delete cfg.ok;",
            "Object.assign(cfg, extra);",
            "Object.defineProperty(cfg, \"ok\", d);",
        ] {
            assert!(written(text, false), "{text}");
        }
        for text in [
            "return { ...cfg, ok: cfg.ok };",
            "if (cfg.ok === true) { return cfg.name; }",
            "const n = cfg.count + 1;",
            "log(\"cfg.ok = false\");",
            "other.cfg = 1;",
            "normalize(cfg);",
            "check({ ...cfg });",
        ] {
            assert!(!written(text, false), "{text}");
        }
        assert!(written("normalize(cfg);", true));
        assert!(written("normalize(a, cfg);", true));
        assert!(!written("check({ ...cfg });", true));
        assert!(!written("if (cfg) { return 1; }", true));
    }

    #[test]
    fn reads_a_test_local_bound_once_to_an_object_literal() {
        let body = "const cfg = { name: \"bo\", ok: true };\n  expect(markOk(cfg)).toEqual(cfg);";
        assert_eq!(
            ts_test_local_object(body, "cfg", &["markOk"]).as_deref(),
            Some("{ name: \"bo\", ok: true }")
        );
        // The same body names another owner, so `markOk(cfg)` is a helper.
        assert_eq!(ts_test_local_object(body, "cfg", &["ready"]), None);
        assert_eq!(
            ts_test_local_object("const cfg: Settings = { ok: true };", "cfg", &["ready"]).as_deref(),
            Some("{ ok: true }")
        );
        assert_eq!(
            ts_test_local_object(
                "const cfg: Map<string, () => boolean> = { ok: true };",
                "cfg",
                &["ready"]
            )
            .as_deref(),
            Some("{ ok: true }")
        );
        for body in [
            "let cfg = { ok: true };",
            "const cfg = load();",
            "const cfg = { ok: true }; { const cfg = { ok: false }; }",
            "var cfg = { ok: true };",
            // a member written later, or the binding handed to a mutator
            "const cfg = { ok: true }; cfg.ok = false;",
            "const cfg = { ok: true }; cfg[\"ok\"] ||= false;",
            "const cfg = { ok: true }; delete cfg.ok;",
            "const cfg = { ok: true }; Object.assign(cfg, { ok: false });",
            // a callback parameter of the same name shadows the binding
            "const cfg = { ok: true }; [{}].forEach((cfg) => check(ready(cfg)));",
            "const cfg = { ok: true }; [{}].forEach(cfg => check(ready(cfg)));",
            "const cfg = { ok: true }; [{}].forEach(function (cfg) { check(ready(cfg)); });",
            "const cfg = { ok: true }; [{}].forEach(({ ...cfg }) => check(ready(cfg)));",
            "const cfg = { ok: true }; try { x(); } catch (cfg) { ready(cfg); }",
            // any use but a read or a whole owner or matcher argument
            "const cfg = { ok: true }; (cfg as any).ok = false;",
            "const cfg = { ok: true }; withOk(cfg);",
            "const cfg = { ok: true }; Reflect.set(cfg, \"ok\", false);",
            "const cfg = { ok: true }; const c = cfg; c.ok = false;",
            "const cfg = { ok: true }; if (cfg.ok == true) { log(cfg); }",
            "const cfg = { ok: true }; expect(ready([cfg][0])).toEqual(cfg);",
        ] {
            assert_eq!(ts_test_local_object(body, "cfg", &["ready"]), None, "{body}");
        }
        assert_eq!(
            ts_test_local_object("const cfgs = { ok: true };", "cfg", &["ready"]),
            None
        );
        // Reading a member, a whole owner or matcher argument, a spread
        // copy, or a ternary after a call keeps the read.
        for body in [
            "const cfg = { ok: true }; expect(ready(cfg).ok).toBe(cfg.ok);",
            "const cfg = { ok: true }; const r = flag ? ready(cfg) : null;",
            "const cfg = { ok: true }; expect(ns.ready(cfg)).toEqual({ ...cfg, ok: true });",
            "const cfg = { ok: true }; expect(await ready(cfg, 1)).toStrictEqual(cfg);",
        ] {
            assert_eq!(
                ts_test_local_object(body, "cfg", &["ready"]).as_deref(),
                Some("{ ok: true }"),
                "{body}"
            );
        }
    }
}
