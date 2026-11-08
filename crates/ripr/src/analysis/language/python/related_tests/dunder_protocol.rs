//! Whether a test body syntactically exercises the Python protocol of a dunder
//! method (#13776). A helper that only a dunder of its own class calls through
//! `self.` is reached by a test only when the test runs that dunder by syntax:
//! `cache[key] = value` runs `__setitem__`, `len(cache)` runs `__len__`,
//! `with cache:` runs `__enter__`. Constructing the class (`Cache()`) runs only
//! `__init__`, which never reaches the helper through this rule, so a test that
//! merely builds the class stays `no_static_path`.
//!
//! The reading is deliberately small and one-sided: a dunder with no
//! syntactic protocol (`__rich_console__`, `__init__`, `__new__`, `__repr__`,
//! `__call__`) never satisfies it. A match grants no relation, oracle or
//! `exposed`; it only keeps the helper from claiming that no test reaches it.

use super::{
    is_python_identifier_char, line_prefix_before, python_callee_start_has_boundary,
    python_text_hides_code,
};

/// Whether `body` uses the syntax that runs at least one of `dunders`.
pub(in crate::analysis::language::python) fn body_exercises_dunder_protocol(
    body: &str,
    dunders: &[String],
) -> bool {
    dunders
        .iter()
        .any(|dunder| exercises(body, dunder.as_str()))
}

fn exercises(body: &str, dunder: &str) -> bool {
    match dunder {
        "__getitem__" => subscripts(body).any(|site| site == Subscript::Read || site.augmented()),
        "__setitem__" => subscripts(body).any(|site| site.assigns()),
        "__delitem__" => subscripts(body).any(|site| site == Subscript::Delete),
        "__contains__" => in_operators(body).any(|is_loop| !is_loop),
        "__iter__" => in_operators(body).any(|is_loop| is_loop) || calls_builtin(body, "iter"),
        "__next__" => calls_builtin(body, "next"),
        "__len__" => calls_builtin(body, "len"),
        "__hash__" => calls_builtin(body, "hash"),
        "__str__" => calls_builtin(body, "str"),
        "__enter__" | "__exit__" => has_with_statement(body),
        "__eq__" | "__ne__" => has_equality_operator(body),
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Subscript {
    Read,
    Assign,
    /// `x[k] += 1`: reads then writes.
    Augmented,
    Delete,
}

impl Subscript {
    fn assigns(self) -> bool {
        matches!(self, Self::Assign | Self::Augmented)
    }

    fn augmented(self) -> bool {
        self == Self::Augmented
    }
}

const NON_VALUE_KEYWORDS: [&str; 17] = [
    "in", "not", "and", "or", "is", "if", "elif", "else", "return", "yield", "assert", "del",
    "lambda", "await", "for", "while", "with",
];

fn trailing_word(text: &str) -> &str {
    let start = text
        .char_indices()
        .rev()
        .find(|(_, ch)| !is_python_identifier_char(*ch))
        .map_or(0, |(at, ch)| at + ch.len_utf8());
    &text[start..]
}

/// Every subscript `value[...]` in code: a `[` that directly follows a value
/// (an identifier that is not a keyword, `)` or `]`), not a list literal.
fn subscripts(body: &str) -> impl Iterator<Item = Subscript> + '_ {
    body.match_indices('[').filter_map(move |(idx, _)| {
        if python_text_hides_code(body, idx) {
            return None;
        }
        let before = &body[..idx];
        let follows_value = match before.chars().next_back()? {
            ')' | ']' => true,
            ch if is_python_identifier_char(ch) => {
                !NON_VALUE_KEYWORDS.contains(&trailing_word(before))
            }
            _ => false,
        };
        if !follows_value {
            return None;
        }
        let close = matching_bracket(body, idx)?;
        let after = body[close + 1..].trim_start();
        let line = line_prefix_before(body, idx).trim_start();
        if line.starts_with("del ") {
            return Some(Subscript::Delete);
        }
        let augmented = ["+=", "-=", "*=", "/=", "//=", "%=", "|=", "&=", "^="]
            .iter()
            .any(|op| after.starts_with(op));
        if augmented {
            Some(Subscript::Augmented)
        } else if after.starts_with('=') && !after.starts_with("==") {
            Some(Subscript::Assign)
        } else {
            Some(Subscript::Read)
        }
    })
}

/// The `]` that closes the `[` at `open`, skipping nested brackets.
fn matching_bracket(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, ch) in text[open..].char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn word_boundaries(text: &str, idx: usize, word: &str) -> bool {
    python_callee_start_has_boundary(text, idx)
        && !text[idx + word.len()..]
            .chars()
            .next()
            .is_some_and(is_python_identifier_char)
}

/// Each `in` keyword in code: `true` for the `in` of a `for` header or
/// comprehension (iteration), `false` for a membership test.
fn in_operators(body: &str) -> impl Iterator<Item = bool> + '_ {
    body.match_indices("in").filter_map(move |(idx, _)| {
        if !word_boundaries(body, idx, "in") || python_text_hides_code(body, idx) {
            return None;
        }
        let prefix = line_prefix_before(body, idx);
        let is_loop = prefix.match_indices("for").any(|(at, _)| {
            word_boundaries(prefix, at, "for") && !python_text_hides_code(prefix, at)
        });
        Some(is_loop)
    })
}

/// A call `name(` of a builtin, not a method or longer name.
fn calls_builtin(body: &str, name: &str) -> bool {
    let call = format!("{name}(");
    body.match_indices(&call).any(|(idx, _)| {
        python_callee_start_has_boundary(body, idx) && !python_text_hides_code(body, idx)
    })
}

fn has_with_statement(body: &str) -> bool {
    let mut offset = 0usize;
    body.split_inclusive('\n').any(|line| {
        let start = offset + line.len() - line.trim_start().len();
        offset += line.len();
        line.trim_start().starts_with("with ") && !python_text_hides_code(body, start)
    })
}

fn has_equality_operator(body: &str) -> bool {
    ["==", "!="].iter().any(|op| {
        body.match_indices(op).any(|(idx, _)| {
            let previous = body[..idx].chars().next_back();
            let after = body[idx + 2..].chars().next();
            !previous.is_some_and(|ch| matches!(ch, '=' | '!' | '<' | '>'))
                && after != Some('=')
                && !python_text_hides_code(body, idx)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(body: &str, dunder: &str) -> bool {
        body_exercises_dunder_protocol(body, &[dunder.to_string()])
    }

    #[test]
    fn construction_and_protocol_free_dunders_never_match() {
        let body = "table = Table()\nassert table is not None\n";
        for dunder in [
            "__init__",
            "__new__",
            "__repr__",
            "__rich_console__",
            "__call__",
        ] {
            assert!(!hit(body, dunder), "{dunder}");
        }
        assert!(!hit(body, "__getitem__"));
        assert!(!hit(body, "__len__"));
    }

    #[test]
    fn subscript_kinds_map_to_item_dunders() {
        assert!(hit("cache[1] = 1\n", "__setitem__"));
        assert!(!hit("cache[1] = 1\n", "__getitem__"));
        assert!(hit("x = cache[1]\n", "__getitem__"));
        assert!(!hit("x = cache[1]\n", "__setitem__"));
        assert!(hit("del cache[1]\n", "__delitem__"));
        assert!(hit("cache[1] += 1\n", "__getitem__"));
        assert!(hit("cache[1] += 1\n", "__setitem__"));
        // A list literal and a comparison are not subscripts.
        assert!(!hit("items = [1, 2]\nassert x == [1]\n", "__getitem__"));
        assert!(!hit("# cache[1]\n", "__getitem__"));
    }

    #[test]
    fn builtin_and_statement_protocols() {
        assert!(hit("assert len(c) == 1\n", "__len__"));
        assert!(!hit("assert self.len(c)\n", "__len__"));
        assert!(hit("for k in c:\n    pass\n", "__iter__"));
        assert!(!hit("for k in c:\n    pass\n", "__contains__"));
        assert!(hit("assert 1 in c\n", "__contains__"));
        assert!(!hit("assert 1 in c\n", "__iter__"));
        assert!(hit("iter(c)\n", "__iter__"));
        assert!(hit("next(it)\n", "__next__"));
        assert!(hit("with c:\n    pass\n", "__enter__"));
        assert!(hit("assert a == b\n", "__eq__"));
        assert!(hit("assert a != b\n", "__ne__"));
        assert!(!hit("a = b\n", "__eq__"));
        assert!(hit("hash(c)\n", "__hash__"));
        assert!(hit("str(c)\n", "__str__"));
    }
}
