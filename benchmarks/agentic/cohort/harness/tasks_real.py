"""Pinned real-code tasks with upstream test items removed from every Rust input.

The bounded stripper supports the item forms used by these pinned crates. Unknown
test-bearing attributes/items make that task unavailable, rather than admitting
test code as production mutation input. Only src/lib.rs is mutated.
"""
import glob
import os
from pathlib import Path
import re

REAL = {}
UNAVAILABLE = {}

_RAW_STRING = re.compile(r'(?:b|c)?r(#+)?"')
_CHAR = re.compile(r"'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|.)|[^'\\\n])'")
_ATTRIBUTE = re.compile(r"#\s*(!?)\s*\[")
_FN = re.compile(r"(?:pub(?:\s*\([^)]*\))?\s+)?(?:(?:async|unsafe|const)\s+)*fn\b")
_ITEM = re.compile(r"(?:pub(?:\s*\([^)]*\))?\s+)?(mod|use|static|const)\b")


def _masked_source(src):
    """Hide comments/literals while retaining offsets and line breaks for item bounds."""
    out, i = list(src), 0
    while i < len(src):
        start = i
        raw = _RAW_STRING.match(src, i) if i == 0 or not (src[i - 1].isalnum() or src[i - 1] == "_") else None
        char = _CHAR.match(src, i) if src[i] == "'" else None
        if src.startswith("//", i):
            end = src.find("\n", i)
            i = len(src) if end < 0 else end
        elif src.startswith("/*", i):
            depth, i = 1, i + 2
            while i < len(src) and depth:
                if src.startswith("/*", i):
                    depth, i = depth + 1, i + 2
                elif src.startswith("*/", i):
                    depth, i = depth - 1, i + 2
                else:
                    i += 1
            if depth:
                raise ValueError("unterminated Rust block comment")
        elif raw:
            closing = '"' + (raw[1] or "")
            end = src.find(closing, raw.end())
            if end < 0:
                raise ValueError("unterminated Rust raw string")
            i = end + len(closing)
        elif char:
            i = char.end()
        elif src[i] == '"':
            i += 1
            while i < len(src) and src[i] != '"':
                i += 2 if src[i] == "\\" else 1
            if i >= len(src):
                raise ValueError("unterminated Rust string")
            i += 1
        else:
            i += 1
            continue
        out[start:i] = ["\n" if c == "\n" else " " for c in src[start:i]]
    return "".join(out)


def _group_end(mask, start):
    closes = {"(": ")", "[": "]", "{": "}"}
    stack = [closes[mask[start]]]
    for pos in range(start + 1, len(mask)):
        char = mask[pos]
        if char in closes:
            stack.append(closes[char])
        elif char in ")]}":
            if char != stack.pop():
                raise ValueError("unbalanced Rust item delimiters")
            if not stack:
                return pos + 1
    raise ValueError("unterminated Rust item")


def _item_end(mask, start, kind):
    pos = start
    while pos < len(mask):
        char = mask[pos]
        if char in "([{":
            end = _group_end(mask, pos)
            if char == "{" and kind in ("fn", "mod"):
                return end
            pos = end
        elif char == ";":
            return pos + 1
        elif char in ")]}":
            raise ValueError("test attribute has no complete supported item")
        else:
            pos += 1
    raise ValueError("test attribute has no complete supported item")


def strip_tests(src):
    mask, spans, pos = _masked_source(src), [], 0
    while match := _ATTRIBUTE.search(mask, pos):
        start, cursor, attrs = match.start(), match.start(), []
        while attr := _ATTRIBUTE.match(mask, cursor):
            end = _group_end(mask, attr.end() - 1)
            body = mask[attr.end():end - 1]
            compact = re.sub(r"\s+", "", body)
            if re.search(r"\btest\b", body) and compact not in ("test", "cfg(test)"):
                raise ValueError("unsupported test-bearing Rust attribute")
            attrs.append((bool(attr[1]), compact))
            cursor = end
            while cursor < len(mask) and mask[cursor].isspace():
                cursor += 1
        if not any(body in ("test", "cfg(test)") for _, body in attrs):
            pos = cursor
            continue
        if any(inner for inner, _ in attrs):
            raise ValueError("inner test attributes are unsupported")
        item = _ITEM.match(mask, cursor)
        kind = "fn" if _FN.match(mask, cursor) else item[1] if item else None
        if kind is None or (any(body == "test" for _, body in attrs) and kind != "fn"):
            raise ValueError("unsupported Rust test item")
        end = _item_end(mask, cursor, kind)
        spans.append((start, end))
        pos = end
    out, pos = [], 0
    for start, end in spans:
        out.append(src[pos:start])
        pos = end
    out.append(src[pos:])
    return "".join(out)

def real(id, crate_glob, cargo_toml, weak_tests, extra=()):
    dirs = sorted(glob.glob(os.path.expanduser(f"~/.cargo/registry/src/*/{crate_glob}")))
    if not dirs:
        UNAVAILABLE[id] = f"pinned source {crate_glob} is not installed"
        return
    try:
        source = Path(dirs[0]) / "src"
        src = strip_tests((source / "lib.rs").read_text(encoding="utf-8"))
        extra_files = {}
        for name in extra:
            text = (source / name).read_text(encoding="utf-8")
            extra_files[f"src/{name}"] = strip_tests(text) if name.endswith(".rs") else text
    except (OSError, UnicodeError, ValueError) as error:
        UNAVAILABLE[id] = f"pinned source {crate_glob} cannot supply production inputs: {error}"
        return
    REAL[id] = dict(id=id, msg=f"feat: add {crate_glob} sources", base_src="", new_src=src, weak_tests=weak_tests,
                    mutants=None, ref_tests="", base_tests="", extra_files=extra_files, cargo_toml=cargo_toml)

real("rstrsim", "strsim-0.11.1", '[package]\nname = "rstrsim"\nversion = "0.1.0"\nedition = "2021"\n[lib]\npath = "src/lib.rs"\ndoctest = false\n',
'''use rstrsim::*;
#[test]
fn hamming_basic() { assert_eq!(hamming("hamming", "hammers"), Ok(3)); }
#[test]
fn levenshtein_basic() { assert_eq!(levenshtein("kitten", "sitting"), 3); }
#[test]
fn jaro_winkler_in_range() { let s = jaro_winkler("martha", "marhta"); assert!(s > 0.9 && s <= 1.0); }
''')
real("rshlex", "shlex-2.0.1", '[package]\nname = "rshlex"\nversion = "0.1.0"\nedition = "2021"\n[features]\ndefault = ["std"]\nstd = []\n[lib]\npath = "src/lib.rs"\ndoctest = false\n',
'''use rshlex::*;
#[test]
fn split_simple() { assert_eq!(split("a b"), Some(vec!["a".to_string(), "b".to_string()])); }
#[test]
fn unterminated_quote_is_none() { assert_eq!(split("a \\"b"), None); }
''', extra=("bytes.rs",))
real("rpercent", "percent-encoding-2.3.2", '[package]\nname = "rpercent"\nversion = "0.1.0"\nedition = "2021"\n[features]\ndefault = ["alloc", "std"]\nalloc = []\nstd = []\n[lib]\npath = "src/lib.rs"\ndoctest = false\n',
'''use rpercent::*;
#[test]
fn encodes_space() { assert_eq!(utf8_percent_encode("a b", NON_ALPHANUMERIC).to_string(), "a%20b"); }
#[test]
fn decodes_space() { assert_eq!(percent_decode_str("a%20b").decode_utf8().unwrap(), "a b"); }
''', extra=("ascii_set.rs",))
