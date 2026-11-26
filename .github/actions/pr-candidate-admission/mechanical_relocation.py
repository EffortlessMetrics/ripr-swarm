#!/usr/bin/env python3
"""Reproducible mechanical relocation of inline test modules (#15217, #15193).

Development tests leave the published `ripr` and `ripr-contract` source by
moving each inline `#[cfg(test)] mod NAME { BODY }` into the out-of-line file
Rust resolves for `mod NAME;`, which the manifests exclude. Moving a body
de-indents every line, so a relocation PR is all deletions plus additions and
exceeds the ordinary admission cap many times over without changing a single
token of Rust.

This module is both the transform and its verifier:

* `apply` rewrites a checkout: the owner keeps every byte except the module
  body, which becomes `;` (plus `#[path = "..._tests.rs"]` when NAME is neither
  `tests` nor `*_tests`, so the file matches the manifest exclusions); the body
  goes to the resolved file, de-indented one level outside string and comment
  tokens, with relative `include_str!`/`include_bytes!`/`include!` paths
  rewritten for the new directory; the inline-test baseline and path-keyed
  metadata follow the moved tests.
* `verify` re-derives that result from the base tree for exactly the files a
  diff touches and accepts only a head that matches it: owners byte for byte,
  new test files token for token (rustfmt may re-wrap lines and add or drop a
  trailing comma before a closing delimiter, nothing else), the baseline file
  byte for byte, and other metadata line for line under the path substitution.
  Any other change, file or construct is a refusal with a concrete reason.

Admission (candidate_admission.py) honors a validated relocation by letting the
ordinary size caps yield to the relocation ceilings below; every test still
builds and runs in the required run.

Stdlib only. The verifier performs no network access itself; the caller hands
it a `read(path, side)` function.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import posixpath
import re
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable, Iterable

DECLARATION_LINE = "mechanical relocation:"
TRANSFORM_ID = "inline-cfg-test-v1"
RELOCATION_REASON = "mechanical_relocation_admitted"
MAX_RELOCATION_LINES = 40000
MAX_RELOCATION_FILES = 400
SOURCE_ROOTS = ("crates/ripr/src/", "crates/ripr-contract/src/")
BASELINE_PATH = "policy/inline-test-modules.txt"
# Path-keyed metadata that names a test's file. A relocation may only rewrite
# an owner path to its new test file on an existing line here.
METADATA_EXACT = (
    ".ripr/traceability.toml",
    ".ripr/allow-attributes.txt",
    ".ripr/test_intent.toml",
    "policy/process_allowlist.txt",
    "policy/network_allowlist.txt",
    "policy/no-panic-allowlist.toml",
    "policy/local_context_allowlist.txt",
)
METADATA_PREFIXES = (
    (".ripr/traceability.d/", ".toml"),
    ("policy/process_allowlist.d/", ".txt"),
)
# Owners another file loads through `#[path]` or `include!` (xtask, build
# scripts, tests, sibling modules). Rust resolves their `mod name;` children
# from the loader, so relocating one is refused. Regenerated and checked by
# `indirect-owners --check`, read from the base side.
INDIRECT_OWNERS_PATH = "policy/relocation-indirect-owners.txt"
INCLUDE_MACROS = ("include_str", "include_bytes", "include")
REFUSED_MACROS = ("file", "line", "column", "stringify")
# Names whose value depends on the source location (or that alias one), refused
# anywhere in a relocated body: `Location::caller()` and `#[track_caller]`
# report the new file and line. A macro defined elsewhere that expands to
# `line!()` is a documented residual (MERGE_FLOW.md).
REFUSED_NAMES = ("Location", "track_caller")
# Published-source `macro_rules!` definitions that shadow a COMMA_NEUTRAL_MACROS
# name crate-wide, reviewed to stay comma-neutral, pinned by the sha256 of their
# NUL-joined tokens so any edit fails `indirect-owners` until re-reviewed.
# stderr_guard's `eprintln!` forwards `$($arg:tt)*` to `::std::format!`.
REVIEWED_SHADOWS = {
    ("crates/ripr/src/stderr_guard.rs", "eprintln"): "d383baab73d83cdd52cb7ceed4d40612444a02c2a20a07a11ce8d017cdb9ef8e",
}


class Refusal(Exception):
    """A relocation the transform does not reproduce mechanically."""


# --------------------------------------------------------------------------
# Lexing


@dataclass(frozen=True)
class Token:
    kind: str  # ws, comment, str, ident, lifetime, punct
    start: int
    end: int


# Rust's Pattern_White_Space; NBSP or U+3000 is not whitespace to rustc.
_RUST_WS = frozenset("\t\n\x0b\x0c\r \x85\u200e\u200f\u2028\u2029")
_RAW_STR = re.compile(r'(?:b|c)?r(#*)"')
_STR = re.compile(r'(?:b|c)?"')
_CHAR = re.compile(r"b?'(?:\\(?:x[0-9A-Fa-f]{2}|u\{[0-9A-Fa-f_]{1,6}\}|.)|[^\\'\n])'", re.S)
# Identifiers include non-ASCII word characters (`fn façade_x`), so whitespace
# can never split one into tokens that compare equal (`mutéx` vs `mut é x`).
_IDENT = re.compile(r"(?:r#)?[^\W\d]\w*")
_LIFETIME = re.compile(r"'(?:r#)?[^\W\d]\w*")
_NUMBER = re.compile(r"[0-9][0-9A-Za-z_]*(?:\.[0-9][0-9A-Za-z_]*)?")
# Multi-character punctuation, longest first: `a && b` and `a & &b` differ.
_PUNCT = ("<<=", ">>=", "...", "..=", "&&", "||", "::", "->", "=>", "==", "!=", "<=", ">=", "<<", ">>",
          "+=", "-=", "*=", "/=", "%=", "^=", "&=", "|=", "..")


def lex(text: str) -> list[Token]:
    """Rust tokens precise enough for brace matching and literal identity.

    Strings (plain, byte, C, raw with any `#` count), char and byte literals,
    nested block comments and line comments are single tokens, so nothing
    inside them is ever read as code. Unterminated constructs raise Refusal.
    """
    tokens: list[Token] = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if c in _RUST_WS:
            j = i
            while j < n and text[j] in _RUST_WS:
                j += 1
            tokens.append(Token("ws", i, j))
            i = j
            continue
        if text.startswith("//", i):
            j = text.find("\n", i)
            j = n if j < 0 else j
            tokens.append(Token("comment", i, j))
            i = j
            continue
        if text.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if text.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif text.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            if depth:
                raise Refusal("unterminated block comment")
            tokens.append(Token("comment", i, j))
            i = j
            continue
        m = _RAW_STR.match(text, i)
        if m:
            close = '"' + m.group(1)
            j = text.find(close, m.end())
            if j < 0:
                raise Refusal("unterminated raw string")
            tokens.append(Token("str", i, j + len(close)))
            i = j + len(close)
            continue
        m = _STR.match(text, i)
        if m:
            j = m.end()
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            if j >= n:
                raise Refusal("unterminated string")
            tokens.append(Token("str", i, j + 1))
            i = j + 1
            continue
        m = _CHAR.match(text, i)
        if m:
            tokens.append(Token("str", i, m.end()))
            i = m.end()
            continue
        if c == "'":
            m = _LIFETIME.match(text, i)
            if m:
                tokens.append(Token("lifetime", i, m.end()))
                i = m.end()
                continue
        m = _IDENT.match(text, i)
        if m:
            tokens.append(Token("ident", i, m.end()))
            i = m.end()
            continue
        m = _NUMBER.match(text, i)
        if m:
            tokens.append(Token("ident", i, m.end()))
            i = m.end()
            continue
        if ord(c) > 127:
            # Any other non-ASCII character outside a string or comment.
            raise Refusal(f"non-ASCII character {c!r} outside a string or comment")
        width = next((len(p) for p in _PUNCT if text.startswith(p, i)), 1)
        tokens.append(Token("punct", i, i + width))
        i += width
    return tokens


# Macros whose arms treat a trailing comma exactly like its absence; inside
# any other macro invocation (or a `macro_rules!` body) no token is dropped.
COMMA_NEUTRAL_MACROS = frozenset((
    "assert", "assert_eq", "assert_ne", "debug_assert", "debug_assert_eq", "debug_assert_ne", "vec", "format",
    "format_args", "print", "println", "eprint", "eprintln", "write", "writeln", "panic", "matches", "todo",
    "unimplemented", "unreachable",
))
KEYWORDS = frozenset((
    "as", "async", "await", "box", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
    "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while", "yield",
))
_CLOSURE_END = frozenset((",", ")", "]", "}", ";"))
# A block whose first token starts a block-like expression: `{ if a {x} else {y} - 1 }`
# ends the `if` at its `}` as a statement, but `if a {x} else {y} - 1` without the
# braces is one binary expression, so these braces are never dropped.
_BLOCK_LIKE_START = frozenset(("if", "match", "loop", "while", "for", "unsafe", "async", "const", "{", "'"))


# Tokens after which a `|` or `||` can only open a closure, never be binary.
# Operators here never end an operand (`>` and `>>` can close generics, so they are absent).
_CLOSURE_OPENER_PREV = frozenset((
    "=", "(", "[", "{", ",", ";", "=>", ":", "move", "return", "break", "in",
    "&", "&&", "!", "+", "-", "*", "/", "%", "^", "<", "==", "!=", "<=", ">=", "..", "..=",
    "+=", "-=", "*=", "/=", "%=", "^=", "&=", "|=", "<<=", ">>=", "<<",
))


def _closes_closure_params(texts: list[str], bar_k: int) -> bool:
    """Whether the `|`/`||` at `bar_k` closes a closure parameter list (so a
    block after it is the closure body), not a binary `|` or `||`. The opening
    bar must follow a token in _CLOSURE_OPENER_PREV (or start the sequence):
    deciding from "the previous token does not end an operand" mistakes the
    closing bar of `|x: Vec<u8>| y | { e }` (after `>`) for an opener."""
    if texts[bar_k] == "||":
        return bar_k == 0 or texts[bar_k - 1] in _CLOSURE_OPENER_PREV
    depth = 0
    for j in range(bar_k - 1, -1, -1):
        t = texts[j]
        if t in (")", "]", "}"):
            depth += 1
        elif t in ("(", "[", "{"):
            if depth == 0:
                return False
            depth -= 1
        elif depth == 0 and t in (";", "=>", "||"):
            return False
        elif depth == 0 and t == "|":
            return j == 0 or texts[j - 1] in _CLOSURE_OPENER_PREV
    return False


def _block_like_start(texts: list[str], open_k: int) -> bool:
    """A block whose first statement could end at its own `}` or carries an
    attribute: a block-like expression, a label, `#[..]`, or a macro call
    with braces (`m!{x} - 1` is a statement then `-1`, but one binary expression
    unbraced; `m!(x) - 1` is one expression either way)."""
    nxt = texts[open_k + 1] if open_k + 1 < len(texts) else ""
    if nxt in _BLOCK_LIKE_START or nxt.startswith("'") or nxt == "#":
        return True
    j = open_k + 1
    while j + 1 < len(texts) and (_IDENT.fullmatch(texts[j]) or texts[j] == "::"):
        if texts[j + 1] == "!":
            # Only a brace-delimited call ends as a statement; `m!(..) - 1` is one expression either way.
            return j + 2 < len(texts) and texts[j + 2] == "{"
        j += 1
    return False


def _bare(t: str) -> str:
    return t[2:] if t.startswith("r#") else t


def _use_names(raw: list[str]) -> set[str]:
    """Names a `use` item binds or imports as a final segment (`use a::b;`,
    `use a::{b, c as d};` give b, c, d)."""
    names: set[str] = set()
    k = 0
    while k < len(raw):
        if raw[k] == "use":
            j = k + 1
            while j < len(raw) and raw[j] != ";":
                if _IDENT.fullmatch(raw[j]) and (j + 1 >= len(raw) or raw[j + 1] in (";", ",", "}", "as")):
                    names.add(_bare(raw[j]))
                j += 1
            k = j
        k += 1
    return names


def refuse_location(where: str, raw: list[str]) -> None:
    """Refuse tokens whose value depends on the source file, line or column:
    a location macro (also spelled as a raw identifier) or a location-reporting
    API (`Location::caller()`, `#[track_caller]`)."""
    for k, tok in enumerate(raw):
        if _bare(tok) in REFUSED_MACROS and k + 1 < len(raw) and raw[k + 1] == "!":
            raise Refusal(f"{where}: `{tok}!` would change value when moved")
        if tok.startswith("r#") and _bare(tok) in INCLUDE_MACROS:
            raise Refusal(f"{where} spells `{tok}`, which the transform does not re-base")
        if _bare(tok) in REFUSED_NAMES:
            raise Refusal(f"{where} uses `{tok}`, which reports the source location")


def refuse_shadowing(where: str, raw: list[str]) -> None:
    """Refuse a local `macro_rules!` of a COMMA_NEUTRAL_MACROS name (its
    trailing commas would then matter) and any `use` that imports a location,
    include or comma-neutral macro name (an alias the token checks cannot see)."""
    for k, tok in enumerate(raw):
        if tok == "macro_rules" and k + 2 < len(raw) and raw[k + 1] == "!" and _bare(raw[k + 2]) in COMMA_NEUTRAL_MACROS:
            raise Refusal(f"{where} defines `{raw[k + 2]}!`, shadowing a macro whose trailing commas are normalized")
    hit = sorted(_use_names(raw) & (set(REFUSED_MACROS) | set(INCLUDE_MACROS) | COMMA_NEUTRAL_MACROS))
    if hit:
        raise Refusal(f"{where} imports {hit} by name, which may alias a location or comma-sensitive macro")


def _is_name(t: str) -> bool:
    return bool(_IDENT.fullmatch(t)) and t not in KEYWORDS


def normalize(texts: list[str]) -> list[str]:
    """The layout choices rustfmt makes differently once a body is
    de-indented, removed from a token sequence so both sides compare equal:

    * a `,` directly before `)`, `]`, `}` or a generic list's `>` (added or dropped when a list
      changes between one-line and vertical layout), except a lone element's
      comma in a `(..)` that is not a call (a 1-tuple), and anything inside a
      macro invocation not in COMMA_NEUTRAL_MACROS or a `macro_rules!` body;
    * the braces of a closure body block holding one expression, `|..| { e }`
      becoming `|..| e`, only when the `|` closes a closure parameter list (not
      a binary `a | { e }`) and the block ends the closure (followed by `,`,
      `;`, a closer or nothing), so no operator or method can bind to `e`;
    * the braces of a match-arm block holding one expression, `=> { e }`
      becoming `=> e,` (a comma is supplied unless one or the closing `}`
      follows, so the next arm cannot continue `e`).

    Neither block rule applies when `e` starts with a block-like expression
    (`if`, `match`, a loop, `unsafe`, a nested block, ...), whose braces
    change how a following operator parses.
    """
    drop: set[int] = set()
    comma_after: set[int] = set()
    stack: list[tuple[int, bool]] = []  # (opener index, verbatim)
    for k, t in enumerate(texts):
        if t in ("(", "[", "{"):
            verbatim = bool(stack) and stack[-1][1]
            if not verbatim and k >= 2 and texts[k - 1] == "!" and _IDENT.fullmatch(texts[k - 2]):
                verbatim = texts[k - 2] not in COMMA_NEUTRAL_MACROS
            if not verbatim and k >= 3 and texts[k - 2] == "!" and texts[k - 3] == "macro_rules":
                verbatim = True
            stack.append((k, verbatim))
        elif t in (">", ">>") and k > 0 and texts[k - 1] == "," and not (stack and stack[-1][1]):
            drop.add(k - 1)  # `, >` only ends a generic list; no expression has it
        elif t in (")", "]", "}") and stack:
            open_k, verbatim = stack.pop()
            if verbatim:
                continue
            if k - 1 > open_k and texts[k - 1] == ",":
                if t != ")" or _has_top_level_comma(texts, open_k + 1, k - 1) or _is_call_paren(texts, open_k):
                    drop.add(k - 1)
            if (
                t == "}"
                and open_k > 0
                and texts[open_k - 1] in ("|", "||")
                and _closes_closure_params(texts, open_k - 1)
                and not _block_like_start(texts, open_k)
                and k > open_k + 1
                and (k + 1 == len(texts) or texts[k + 1] in _CLOSURE_END)
                and not _has_top_level(texts, open_k + 1, k, ";")
            ):
                drop.update((open_k, k))
            if (
                t == "}"
                and open_k > 0
                and texts[open_k - 1] == "=>"
                and not _block_like_start(texts, open_k)
                and k > open_k + 1
                and not _has_top_level(texts, open_k + 1, k, ";")
            ):
                drop.update((open_k, k))
                if k + 1 < len(texts) and texts[k + 1] not in (",", "}"):
                    comma_after.add(k)
    out: list[str] = []
    for k, t in enumerate(texts):
        if k not in drop:
            out.append(t)
        if k in comma_after:
            out.append(",")
    return out


def _has_top_level(texts: list[str], start: int, end: int, needle: str) -> bool:
    depth = 0
    for inner in texts[start:end]:
        if inner in ("(", "[", "{"):
            depth += 1
        elif inner in (")", "]", "}"):
            depth -= 1
        elif inner == needle and depth == 0:
            return True
    return False


def _has_top_level_comma(texts: list[str], start: int, end: int) -> bool:
    return _has_top_level(texts, start, end, ",")


def _is_call_paren(texts: list[str], open_k: int) -> bool:
    """A `(` that opens arguments (a call, tuple struct or comma-neutral macro),
    where one element's trailing comma is layout, not a 1-tuple."""
    if open_k == 0:
        return False
    prev = texts[open_k - 1]
    if prev == "!":
        return open_k >= 2 and texts[open_k - 2] in COMMA_NEUTRAL_MACROS
    # Not `]`: an attribute's closing bracket precedes a pattern or expression
    # `#[allow(..)] (a,)`, a 1-tuple.
    return _is_name(prev) or prev == ")"


def _is_doc_comment(c: str) -> bool:
    return (c.startswith("///") and not c.startswith("////")) or c.startswith("//!") or (
        (c.startswith("/**") and not c.startswith("/***") and c != "/**/") or c.startswith("/*!")
    )


def raw_tokens(text: str) -> list[str]:
    """Token texts without whitespace and ordinary comments. A doc comment is
    an attribute on the item that follows it, so it stays in the sequence (its
    content trimmed line by line) and cannot move to another item."""
    out = []
    for t in lex(text):
        if t.kind == "ws":
            continue
        word = text[t.start : t.end]
        if t.kind == "comment":
            if _is_doc_comment(word):
                out.append("\n".join(line.strip() for line in word.split("\n")))
            continue
        out.append(word)
    return out


def significant(text: str) -> list[str]:
    """Normalized token texts without whitespace and ordinary comments."""
    return normalize(raw_tokens(text))


def comment_texts(text: str) -> list[tuple[int, str, str]]:
    """Comments in order with their indentation-independent content, so a
    relocation cannot drop or edit one (line comments compared trimmed,
    block comments line by line trimmed). Each is anchored by the number of
    identifier, literal and lifetime tokens before it, which normalization
    never adds or removes, and by the next code token, so a comment cannot
    move to another item: a `// covered-by:` marker above `#[test] fn b`
    cannot reattach to `fn b`'s neighbour, even at the end of `fn a`'s body
    (next token `}`, not `#`)."""
    toks = lex(text)
    out = []
    words = 0
    for k, t in enumerate(toks):
        if t.kind == "comment":
            nxt = next((text[u.start : u.end] for u in toks[k + 1 :] if u.kind not in ("ws", "comment")), "")
            out.append((words, nxt, "\n".join(line.strip() for line in text[t.start : t.end].split("\n"))))
        elif t.kind in ("ident", "str", "lifetime"):
            words += 1
    return out


# --------------------------------------------------------------------------
# Module discovery


@dataclass
class InlineModule:
    name: str
    attrs: list[str]
    item_start: int  # first attribute
    keyword_line_start: int  # offset of the line holding `mod` (or visibility)
    open_brace: int
    close_brace: int


def _skip_group(sig: list[Token], text: str, k: int, open_c: str, close_c: str) -> int:
    depth = 0
    while k < len(sig):
        x = text[sig[k].start : sig[k].end]
        if x == open_c:
            depth += 1
        elif x == close_c:
            depth -= 1
            if depth == 0:
                return k
        k += 1
    raise Refusal(f"unbalanced {open_c}{close_c}")


_TEST_ONLY_CFG = re.compile(r"#\s*\[\s*cfg\s*\((.*)\)\s*\]\s*", re.S)


def is_test_only_cfg(attr: str) -> bool:
    """`#[cfg(test)]` or `#[cfg(all(.., test, ..))]` with `test` at the top
    level of the `all`. `any(test, ..)` compiles outside tests and `not(test)`
    is production, so neither is a test-only gate."""
    m = _TEST_ONLY_CFG.fullmatch(attr.strip())
    if not m:
        return False
    # Tokenized, so a `)` or `,` inside a string literal is not structure.
    pred = raw_tokens(m.group(1))
    if pred == ["test"]:
        return True
    if len(pred) < 4 or pred[:2] != ["all", "("] or pred[-1] != ")":
        return False
    depth, part, parts = 0, [], []
    for tok in pred[2:-1]:
        if tok in ("(", "["):
            depth += 1
        elif tok in (")", "]"):
            depth -= 1
            if depth < 0:
                return False  # `all(a) , test (b)`: the `all` closed early
        if tok == "," and depth == 0:
            parts.append(part)
            part = []
        else:
            part.append(tok)
    parts.append(part)
    return depth == 0 and ["test"] in parts


def top_level_test_modules(text: str) -> list[InlineModule]:
    """Top-level inline modules whose outer attributes include a test-only
    cfg. Nested modules are not relocated by this transform."""
    sig = [t for t in lex(text) if t.kind not in ("ws", "comment")]
    s = lambda t: text[t.start : t.end]  # noqa: E731
    found: list[InlineModule] = []
    depth = 0
    k = 0
    while k < len(sig):
        x = s(sig[k])
        if x in ("{", "(", "["):
            depth += 1
            k += 1
            continue
        if x in ("}", ")", "]"):
            depth -= 1
            k += 1
            continue
        if depth != 0 or x != "#" or k + 1 >= len(sig) or s(sig[k + 1]) != "[":
            k += 1
            continue
        item_start = sig[k].start
        attrs: list[str] = []
        while k + 1 < len(sig) and s(sig[k]) == "#" and s(sig[k + 1]) == "[":
            end = _skip_group(sig, text, k + 1, "[", "]")
            attrs.append(text[sig[k].start : sig[end].end])
            k = end + 1
        m = k
        if m < len(sig) and s(sig[m]) == "pub":
            m += 1
            if m < len(sig) and s(sig[m]) == "(":
                m = _skip_group(sig, text, m, "(", ")") + 1
        if (
            m + 2 < len(sig)
            and s(sig[m]) == "mod"
            and sig[m + 1].kind == "ident"
            and s(sig[m + 2]) == "{"
            and any(is_test_only_cfg(a) for a in attrs)
        ):
            close = _skip_group(sig, text, m + 2, "{", "}")
            keyword_tok = sig[k] if k < m else sig[m]
            found.append(
                InlineModule(
                    name=s(sig[m + 1]),
                    attrs=attrs,
                    item_start=item_start,
                    keyword_line_start=text.rfind("\n", 0, keyword_tok.start) + 1,
                    open_brace=sig[m + 2].start,
                    close_brace=sig[close].start,
                )
            )
            k = close + 1
            continue
        # Attributes on some other item: let the depth tracking see its body.
    return found


# --------------------------------------------------------------------------
# Paths


def owner_base(owner: str) -> str:
    """Directory Rust resolves `mod NAME;` children of `owner` against."""
    d, f = posixpath.split(owner)
    stem = f[:-3]
    crate_root = stem in ("lib", "main") and (d == "src" or d.endswith("/src"))
    return d if (stem == "mod" or crate_root) else posixpath.join(d, stem)


def is_test_name(name: str) -> bool:
    return name == "tests" or name.endswith("_tests")


def target_for(owner: str, name: str) -> tuple[str, str | None]:
    """(target file, `#[path]` value or None) for relocating module `name`."""
    bare = name[2:] if name.startswith("r#") else name
    if is_test_name(bare):
        return posixpath.join(owner_base(owner), f"{bare}.rs"), None
    file = posixpath.join(owner_base(owner), f"{bare}_tests.rs")
    return file, posixpath.relpath(file, posixpath.dirname(owner))


def in_scope(path: str) -> bool:
    return path.endswith(".rs") and path.startswith(SOURCE_ROOTS)


def is_metadata(path: str) -> bool:
    return path in METADATA_EXACT or any(path.startswith(p) and path.endswith(s) for p, s in METADATA_PREFIXES)


# --------------------------------------------------------------------------
# Transform


@dataclass
class Move:
    owner: str
    name: str
    target: str
    path_attr: str | None
    body: str  # text between the braces, as in the base owner
    body_lines: int


def _plain_literal(lit: str) -> str | None:
    if len(lit) >= 2 and lit[0] == '"' and lit[-1] == '"' and "\\" not in lit:
        return lit[1:-1]
    return None


def expected_body_tokens(move: Move) -> list[str]:
    """Body tokens as they must read in the target: relative include paths
    re-based from the owner's directory to the target's."""
    raw = raw_tokens(move.body)
    where = f"{move.owner}: `{move.name}`"
    refuse_location(where, raw)
    refuse_shadowing(where, raw)
    owner_dir = posixpath.dirname(move.owner)
    target_dir = posixpath.dirname(move.target)
    out = list(raw)
    for k, tok in enumerate(raw):
        if tok == "#" and k + 1 < len(raw) and raw[k + 1] in ("[", "!"):
            open_k = k + 1 if raw[k + 1] == "[" else k + 2
            depth, j = 0, open_k
            while j < len(raw):
                depth += raw[j] == "["
                depth -= raw[j] == "]"
                if depth == 0:
                    break
                j += 1
            if any(_bare(t) == "path" for t in raw[open_k:j]):
                raise Refusal(f"{move.owner}: a `path` attribute inside `{move.name}` would resolve from a new base")
        if tok in INCLUDE_MACROS and k + 2 < len(raw) and raw[k + 1] == "!" and raw[k + 2] in ("[", "{"):
            raise Refusal(f"{move.owner}: `{tok}!` inside `{move.name}` uses a delimiter the transform does not re-base")
        if tok in INCLUDE_MACROS and k + 3 < len(raw) and raw[k + 1] == "!" and raw[k + 2] == "(":
            if raw[k + 3 : k + 10] == ["concat", "!", "(", "env", "!", "(", '"CARGO_MANIFEST_DIR"']:
                continue  # anchored at the manifest, independent of the file's location
            value = _plain_literal(raw[k + 3])
            if value is None:
                raise Refusal(f"{move.owner}: `{tok}!` inside `{move.name}` does not take one plain string literal")
            if value.startswith("/"):
                continue
            resolved = posixpath.normpath(posixpath.join(owner_dir, value))
            out[k + 3] = '"' + posixpath.relpath(resolved, target_dir) + '"'
    if move.path_attr is not None:
        for k, tok in enumerate(raw):
            if tok == "mod" and k + 2 < len(raw) and raw[k + 2] == ";":
                raise Refusal(
                    f"{move.owner}: `{move.name}` declares an out-of-line child, which a `#[path]` file resolves beside itself"
                )
    return normalize(out)


def dedent(body: str) -> str:
    """Strip one 4-space level from each line not starting inside a string or
    comment token, so literal contents keep every byte."""
    inside: set[int] = set()
    for t in lex(body):
        if t.kind in ("str", "comment"):
            p = body.find("\n", t.start, t.end)
            while p != -1:
                inside.add(p + 1)
                p = body.find("\n", p + 1, t.end)
    out, pos = [], 0
    for line in body.split("\n"):
        out.append(line[4:] if pos not in inside and line.startswith("    ") else line)
        pos += len(line) + 1
    return "\n".join(out)


def rewrite_includes(text: str, move: Move) -> str:
    owner_dir = posixpath.dirname(move.owner)
    target_dir = posixpath.dirname(move.target)
    toks = [t for t in lex(text) if t.kind not in ("ws", "comment")]
    edits = []
    for k, t in enumerate(toks):
        word = text[t.start : t.end]
        if word in INCLUDE_MACROS and k + 3 < len(toks) and text[toks[k + 1].start : toks[k + 1].end] == "!":
            lit = toks[k + 3]
            value = _plain_literal(text[lit.start : lit.end])
            if value is None or value.startswith("/"):
                continue
            resolved = posixpath.normpath(posixpath.join(owner_dir, value))
            edits.append((lit.start, lit.end, '"' + posixpath.relpath(resolved, target_dir) + '"'))
    for start, end, new in reversed(edits):
        text = text[:start] + new + text[end:]
    return text


def plan_moves(owner: str, text: str, names: Iterable[str] | None = None) -> list[Move]:
    if not in_scope(owner):
        raise Refusal(f"{owner} is outside the published source roots")
    modules = top_level_test_modules(text)
    wanted = None if names is None else set(names)
    moves = []
    if modules:
        # The owner's macro scope reaches the body (`use super::*`), and every
        # line after the first relocated body moves up.
        refuse_shadowing(owner, raw_tokens(text))
        refuse_location(f"{owner} after its first test module", raw_tokens(text[modules[0].open_brace :]))
    for m in modules:
        if wanted is not None and m.name not in wanted:
            continue
        if any(_bare(tok) == "path" for a in m.attrs for tok in raw_tokens(a)):
            raise Refusal(f"{owner}: `{m.name}` carries a `path` attribute, so it does not resolve where the transform places it")
        target, path_attr = target_for(owner, m.name)
        body = text[m.open_brace + 1 : m.close_brace]
        moves.append(Move(owner, m.name, target, path_attr, body, body.count("\n")))
    return moves


def owner_after(text: str, owner: str, names: set[str]) -> str:
    """The owner with each named top-level test module's `{ BODY }` replaced
    by `;` (and a `#[path]` line added when the name needs one)."""
    modules = [m for m in top_level_test_modules(text) if m.name in names]
    if {m.name for m in modules} != names:
        raise Refusal(f"{owner}: no top-level inline test module named {sorted(names - {m.name for m in modules})}")
    for m in sorted(modules, key=lambda m: m.open_brace, reverse=True):
        _, path_attr = target_for(owner, m.name)
        head = text[: m.open_brace].rstrip(" ")
        text = head + ";" + text[m.close_brace + 1 :]
        if path_attr is not None:
            indent = re.match(r"[ \t]*", text[m.keyword_line_start :]).group(0)
            line = f'{indent}#[path = "{path_attr}"]\n'
            text = text[: m.keyword_line_start] + line + text[m.keyword_line_start :]
    return text


def target_text(move: Move) -> str:
    body = move.body[1:] if move.body.startswith("\n") else move.body
    return rewrite_includes(dedent(body).rstrip() + "\n", move)


def baseline_after(text: str, removed: dict[str, int]) -> str:
    out = []
    seen = set()
    for line in text.splitlines(keepends=True):
        stripped = line.rstrip("\n")
        if stripped and not stripped.startswith("#") and "|" in stripped:
            path, count = stripped.split("|", 1)
            if path in removed:
                seen.add(path)
                left = int(count) - removed[path]
                if left < 0:
                    raise Refusal(f"{BASELINE_PATH}: {path} would fall below zero")
                if left == 0:
                    continue
                line = f"{path}|{left}\n"
        out.append(line)
    missing = set(removed) - seen
    if missing:
        raise Refusal(f"{BASELINE_PATH} has no row for {sorted(missing)}")
    return "".join(out)


_FN = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)")


def rewrite_metadata(text: str, moves: list[Move], owners_after: dict[str, str]) -> str:
    """Re-key a metadata file for `moves`, the one rewrite both `apply` and
    the verifier use: a quoted `"OWNER::NAME::rest"` reference becomes
    `"TARGET::rest"`; a quoted `"OWNER::fn"` follows a function defined only
    in the moved body (not in the owner afterwards); an `OWNER|pattern|..`
    row moves when its pattern occurs in the moved body and no longer in the
    owner. Nothing else changes."""
    moved_fns = {
        id(m): sorted(set(_FN.findall(m.body)) - set(_FN.findall(owners_after[m.owner]))) for m in moves
    }
    out = []
    for line in text.splitlines(keepends=True):
        new = line
        for m in moves:
            if m.owner not in new:
                continue
            after = owners_after[m.owner]
            new = new.replace(f'"{m.owner}::{m.name}::', f'"{m.target}::')
            for fn in moved_fns[id(m)]:
                new = new.replace(f'"{m.owner}::{fn}"', f'"{m.target}::{fn}"')
            if new.startswith(f"{m.owner}|"):
                parts = new.split("|")
                if len(parts) >= 2 and parts[1] and parts[1] in m.body and parts[1] not in after:
                    new = m.target + new[len(m.owner) :]
        out.append(new)
    return "".join(_rekey_test_intent(out, moves, moved_fns))


_INTENT_TEST = re.compile(r'\s*test\s*=\s*"([^"\\]+)"\s*')
_INTENT_PATH = re.compile(r'(\s*path\s*=\s*)"([^"\\]+)"(\s*)')


def _rekey_test_intent(lines: list[str], moves: list[Move], moved_fns: dict[int, list[str]]) -> list[str]:
    """A `[[table]]` block naming a test by separate `test = "fn"` and
    `path = "OWNER"` keys (`.ripr/test_intent.toml`) follows its test: the
    path becomes the target when exactly one relocated module of OWNER
    defines `fn` (and the owner no longer does). Other blocks are unchanged."""
    out = list(lines)
    starts = [k for k, line in enumerate(out) if line.lstrip().startswith("[")] + [len(out)]
    if starts[0] != 0:
        starts.insert(0, 0)
    for a, b in zip(starts, starts[1:]):
        tests = [m.group(1) for line in out[a:b] if (m := _INTENT_TEST.fullmatch(line.rstrip("\n")))]
        paths = [k for k in range(a, b) if _INTENT_PATH.fullmatch(out[k].rstrip("\n"))]
        if len(tests) != 1 or len(paths) != 1:
            continue
        k = paths[0]
        pm = _INTENT_PATH.fullmatch(out[k].rstrip("\n"))
        assert pm is not None
        hits = [m for m in moves if m.owner == pm.group(2) and tests[0] in moved_fns[id(m)]]
        if len(hits) > 1:
            raise Refusal(f"`{tests[0]}` moves from {pm.group(2)} in more than one module")
        if hits:
            eol = out[k][len(out[k].rstrip("\n")) :]
            out[k] = f'{pm.group(1)}"{hits[0].target}"{pm.group(3)}{eol}'
    return out


def indirect_after(text: str, moves: list[Move]) -> str:
    """INDIRECT_OWNERS_PATH after `moves`: a module relocated under its own
    name is loaded through the transform's `#[path]`, so its target joins the
    list (sorted, as `indirect-owners --write` prints it)."""
    named = {m.target for m in moves if m.path_attr is not None}
    if not named:
        return text
    listed = {line.strip() for line in text.splitlines() if line.strip() and not line.startswith("#")}
    return INDIRECT_OWNERS_HEADER + "".join(f"{o}\n" for o in sorted(listed | named))


# --------------------------------------------------------------------------
# Verification


@dataclass
class Verdict:
    status: str  # validated | rejected | not_requested
    detail: str = ""
    moves: list[dict[str, object]] = field(default_factory=list)

    def record(self) -> dict[str, object]:
        return {"status": self.status, "detail": self.detail, "transform": TRANSFORM_ID, "moves": self.moves}


def declared(body: str) -> str | None:
    """The declared transform id, when the body carries exactly one
    `Mechanical relocation:` line."""
    values = [
        line.strip()[len(DECLARATION_LINE) :].strip().strip("`").strip()
        for line in (body or "").splitlines()
        if line.strip().casefold().startswith(DECLARATION_LINE)
    ]
    if len(values) != 1:
        return None if not values else ""
    return values[0]


def git_blob_sha(text: str) -> str:
    data = text.encode("utf-8")
    return hashlib.sha1(b"blob %d\0" % len(data) + data).hexdigest()


def verify(
    changed: list[dict[str, str]], read: Callable[[str, str], str | None], require_blob_sha: bool = False
) -> Verdict:
    """Accept `changed` (GitHub file entries: filename, status,
    previous_filename) only if it is exactly the relocation the transform
    reproduces from the base side. `read(path, "base"|"head")` returns text
    or None when the path is absent on that side; it raises on read failure.
    With `require_blob_sha`, every entry's `sha` must be the git blob id of the
    head text read, so a listing taken at another head cannot pass."""
    try:
        return _verify(changed, read, require_blob_sha)
    except Refusal as why:
        return Verdict("rejected", str(why))


def _verify(changed: list[dict[str, str]], read: Callable[[str, str], str | None], require_blob_sha: bool) -> Verdict:
    if not changed:
        raise Refusal("the diff lists no files")
    indirect_text = read(INDIRECT_OWNERS_PATH, "base")
    if indirect_text is None:
        raise Refusal(f"{INDIRECT_OWNERS_PATH} is missing at the base")
    indirect = {line.strip() for line in indirect_text.splitlines() if line.strip() and not line.startswith("#")}
    owners, added, metadata = [], set(), []
    for entry in changed:
        path = str(entry.get("filename") or "")
        status = str(entry.get("status") or "")
        if entry.get("previous_filename") and entry.get("previous_filename") != path:
            raise Refusal(f"{path} is a rename, which a relocation never makes")
        if in_scope(path) and status == "modified":
            owners.append(path)
        elif in_scope(path) and status == "added":
            added.add(path)
        elif path in (BASELINE_PATH, INDIRECT_OWNERS_PATH) and status == "modified":
            pass
        elif is_metadata(path) and status == "modified":
            metadata.append(path)
        else:
            raise Refusal(f"{path} ({status}) is not an owner, a new test file or relocation metadata")
    if not owners:
        raise Refusal("no owner file changed")

    moves: list[Move] = []
    owners_after: dict[str, str] = {}
    for owner in sorted(owners):
        base = read(owner, "base")
        head = read(owner, "head")
        if base is None or head is None:
            raise Refusal(f"{owner} is missing on one side")
        if owner in indirect:
            raise Refusal(f"{owner} is loaded through `#[path]` or `include!` elsewhere ({INDIRECT_OWNERS_PATH})")
        if "/src/bin/" in owner or posixpath.basename(owner) == "build.rs":
            raise Refusal(f"{owner} is a crate root the transform does not model")
        base_names = {m.name for m in top_level_test_modules(base)}
        head_names = {m.name for m in top_level_test_modules(head)}
        if not head_names <= base_names:
            raise Refusal(f"{owner} gained an inline test module")
        gone = base_names - head_names
        if not gone:
            raise Refusal(f"{owner} changed but relocated no inline test module")
        if head != owner_after(base, owner, gone):
            raise Refusal(f"{owner} differs from the transform's owner beyond the relocated module bodies")
        owners_after[owner] = head
        moves.extend(plan_moves(owner, base, gone))

    targets = {m.target for m in moves}
    if len(targets) != len(moves):
        raise Refusal("two relocated modules resolve to the same file")
    if targets != added:
        raise Refusal(
            f"new files {sorted(added - targets)} are not relocation targets; targets {sorted(targets - added)} were not added"
        )
    for m in moves:
        if read(m.target, "base") is not None:
            raise Refusal(f"{m.target} already exists at the base")
        text = read(m.target, "head")
        if text is None:
            raise Refusal(f"{m.target} is missing at the head")
        if significant(text) != expected_body_tokens(m):
            raise Refusal(f"{m.target} is not the token-identical body of {m.owner}::{m.name}")
        if comment_texts(text) != comment_texts(m.body):
            raise Refusal(f"{m.target} does not carry the comments of {m.owner}::{m.name} unchanged")

    if any(str(e.get("filename")) == BASELINE_PATH for e in changed):
        base = read(BASELINE_PATH, "base")
        head = read(BASELINE_PATH, "head")
        removed: dict[str, int] = {}
        for m in moves:
            removed[m.owner] = removed.get(m.owner, 0) + 1
        if base is None or head != baseline_after(base, removed):
            raise Refusal(f"{BASELINE_PATH} is not the base with one ceiling removed per relocated module")
    elif read(BASELINE_PATH, "base") is not None:
        raise Refusal(f"{BASELINE_PATH} must lower the ceilings of the relocated modules")

    indirect_expected = indirect_after(indirect_text, moves)
    if any(str(e.get("filename")) == INDIRECT_OWNERS_PATH for e in changed):
        if indirect_expected == indirect_text or read(INDIRECT_OWNERS_PATH, "head") != indirect_expected:
            raise Refusal(f"{INDIRECT_OWNERS_PATH} is not the base plus the targets loaded through the transform's `#[path]`")
    elif indirect_expected != indirect_text:
        raise Refusal(f"{INDIRECT_OWNERS_PATH} must list the targets loaded through the transform's `#[path]`")

    for path in metadata:
        base = read(path, "base")
        head = read(path, "head")
        if base is None or head is None:
            raise Refusal(f"{path} is missing on one side")
        if head != rewrite_metadata(base, moves, owners_after):
            raise Refusal(f"{path} is not the base re-keyed for the relocated tests")

    if require_blob_sha:
        for entry in changed:
            path = str(entry.get("filename") or "")
            text = read(path, "head")
            if text is None or str(entry.get("sha") or "") != git_blob_sha(text):
                raise Refusal(f"{path}: the listed blob is not the head content read")

    return Verdict(
        "validated",
        "",
        [{"owner": m.owner, "module": m.name, "target": m.target, "lines": m.body_lines} for m in moves],
    )


# --------------------------------------------------------------------------
# Local CLI: apply to a checkout, or verify two git revisions.


def git_reader(repo: Path, base: str, head: str) -> Callable[[str, str], str | None]:
    def read(path: str, side: str) -> str | None:
        ref = base if side == "base" else head
        if ref == "WORKTREE":
            p = repo / path
            return p.read_text(encoding="utf-8") if p.exists() else None
        done = subprocess.run(["git", "-C", str(repo), "show", f"{ref}:{path}"], capture_output=True)
        if done.returncode != 0:
            return None
        return done.stdout.decode("utf-8")

    return read


def git_changed(repo: Path, base: str, head: str) -> list[dict[str, str]]:
    args = ["git", "-C", str(repo), "diff", "--no-renames", "--name-status", base]
    if head != "WORKTREE":
        args.append(head)
    out = subprocess.run(args, capture_output=True, text=True, check=True).stdout
    if head == "WORKTREE":
        untracked = subprocess.run(
            ["git", "-C", str(repo), "ls-files", "--others", "--exclude-standard"], capture_output=True, text=True, check=True
        ).stdout
        out += "".join(f"A\t{p}\n" for p in untracked.splitlines())
    status = {"M": "modified", "A": "added", "D": "removed"}
    return [
        {"filename": line.split("\t", 1)[1], "status": status.get(line[0], line[0])}
        for line in out.splitlines()
        if line.strip()
    ]


def apply(repo: Path, owners: list[str], rustfmt: bool = True) -> list[Move]:
    """Relocate every top-level inline test module of each owner. Every owner
    is planned and checked before anything is written, so a refusal leaves the
    checkout untouched."""
    moves: list[Move] = []
    writes: list[tuple[Path, str]] = []
    indirect_file = repo / INDIRECT_OWNERS_PATH
    indirect = set(indirect_file.read_text(encoding="utf-8").split()) if indirect_file.exists() else set()
    for owner in owners:
        if owner in indirect or "/src/bin/" in owner or posixpath.basename(owner) == "build.rs":
            raise Refusal(f"{owner} is loaded indirectly or is an unmodelled crate root")
        path = repo / owner
        text = path.read_text(encoding="utf-8")
        planned = plan_moves(owner, text)
        if not planned:
            raise Refusal(f"{owner} has no top-level inline test module")
        for m in planned:
            expected_body_tokens(m)  # refuses unreproducible bodies before writing
            if (repo / m.target).exists():
                raise Refusal(f"{m.target} already exists")
        writes.append((path, owner_after(text, owner, {m.name for m in planned})))
        writes.extend((repo / m.target, target_text(m)) for m in planned)
        moves.extend(planned)
    if len({m.target for m in moves}) != len(moves):
        raise Refusal("two relocated modules resolve to the same file")
    for path, text in writes:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
    if rustfmt and moves:
        subprocess.run(["rustfmt", "--edition", "2024", *[str(repo / m.target) for m in moves]], check=True)
    removed: dict[str, int] = {}
    for m in moves:
        removed[m.owner] = removed.get(m.owner, 0) + 1
    baseline = repo / BASELINE_PATH
    if baseline.exists():
        baseline.write_text(baseline_after(baseline.read_text(encoding="utf-8"), removed), encoding="utf-8")
    if indirect_file.exists():
        before = indirect_file.read_text(encoding="utf-8")
        after = indirect_after(before, moves)
        if after != before:
            indirect_file.write_text(after, encoding="utf-8")
    owners_after = {m.owner: (repo / m.owner).read_text(encoding="utf-8") for m in moves}
    meta_files = [repo / p for p in METADATA_EXACT]
    for prefix, suffix in METADATA_PREFIXES:
        meta_files += sorted((repo / prefix).glob(f"*{suffix}")) if (repo / prefix).is_dir() else []
    for meta in meta_files:
        if meta.exists():
            before = meta.read_text(encoding="utf-8")
            after = rewrite_metadata(before, moves, owners_after)
            if after != before:
                meta.write_text(after, encoding="utf-8")
    return moves


def _literal_value(tok: str) -> str | None:
    """The value of a plain or raw string literal token without escapes."""
    plain = _plain_literal(tok)
    if plain is not None:
        return plain
    m = re.fullmatch(r'r(#*)"(.*)"\1', tok, re.S)
    return m.group(2) if m else None


def indirect_loads(path: str, text: str, crate_dir: str) -> tuple[set[str], list[str]]:
    """Files `path` loads other than by ordinary `mod name;` resolution, and
    the load forms it could not resolve. Loads: each `path = "<lit>"` (any
    `#[path]` or `cfg_attr(.., path = ..)`) resolved from both the file's
    directory and its child directory (the inline-module case), and each
    `include!("<lit>")` or `include!(concat!(env!("CARGO_MANIFEST_DIR"),
    "<lit>"))` (`include_str!` and `include_bytes!` read data, not modules).
    Unresolved: a `path =` inside an attribute, or an `include!`, whose
    argument is not one escape-free literal (a macro fragment, an escaped
    string, another expression), and a `macro_rules!` of a COMMA_NEUTRAL_MACROS
    name. Over-inclusion only refuses more owners; an unresolved form fails
    `indirect-owners`, so the list can never silently omit a loaded file."""
    raw = raw_tokens(text)
    here = posixpath.dirname(path)
    child = owner_base(path) if path.endswith(".rs") else here
    found: set[str] = set()
    unresolved: list[str] = []
    attr_depth: list[int] = []  # bracket depths at which an attribute opened
    depth = 0
    for k, tok in enumerate(raw):
        if tok == "[":
            if k > 0 and (raw[k - 1] == "#" or (raw[k - 1] == "!" and k > 1 and raw[k - 2] == "#")):
                attr_depth.append(depth)
            depth += 1
        elif tok == "]":
            depth -= 1
            if attr_depth and attr_depth[-1] == depth:
                attr_depth.pop()
        if _bare(tok) == "path" and k + 2 < len(raw) and raw[k + 1] == "=":
            value = _literal_value(raw[k + 2])
            if value is not None:
                found.update(posixpath.normpath(posixpath.join(d, value)) for d in (here, child))
            elif attr_depth:
                unresolved.append(f"{path}: `path = {raw[k + 2]}`")
        if _bare(tok) == "include" and k + 3 < len(raw) and raw[k + 1] == "!" and raw[k + 2] in ("(", "[", "{"):
            if raw[k + 3 : k + 10] == ["concat", "!", "(", "env", "!", "(", '"CARGO_MANIFEST_DIR"'] and k + 12 < len(raw):
                value = _literal_value(raw[k + 12])
                if value is not None and raw[k + 13 : k + 15] == [")", ")"]:
                    found.add(posixpath.normpath(posixpath.join(crate_dir, value.lstrip("/"))))
                    continue
            else:
                value = _literal_value(raw[k + 3])
                if value is not None:
                    found.add(posixpath.normpath(posixpath.join(here, value)))
                    continue
            unresolved.append(f"{path}: `include!({raw[k + 3]} ..)`")
        if (
            path.startswith(SOURCE_ROOTS)
            and tok == "macro_rules"
            and k + 3 < len(raw)
            and raw[k + 1] == "!"
            and _bare(raw[k + 2]) in COMMA_NEUTRAL_MACROS
        ):
            close = k + 3
            group = 0
            while close < len(raw):
                group += raw[close] in ("(", "[", "{")
                group -= raw[close] in (")", "]", "}")
                if group == 0:
                    break
                close += 1
            digest = hashlib.sha256("\0".join(raw[k : close + 1]).encode()).hexdigest()
            if REVIEWED_SHADOWS.get((path, _bare(raw[k + 2]))) != digest:
                unresolved.append(f"{path}: `macro_rules! {raw[k + 2]}` shadows a comma-neutral macro (definition {digest[:16]})")
    if path.startswith(SOURCE_ROOTS):
        for k, tok in enumerate(raw):
            if tok == "as" and k + 1 < len(raw) and _bare(raw[k + 1]) in COMMA_NEUTRAL_MACROS:
                unresolved.append(f"{path}: `as {raw[k + 1]}` may alias another macro to a comma-neutral name")
    return found, unresolved


def indirect_owners(repo: Path) -> tuple[list[str], list[str]]:
    """Tracked published-source files that some tracked `.rs` file loads
    through `#[path]` or `include!` (the INDIRECT_OWNERS_PATH contents), and
    every load form the scan could not resolve (including files it could not
    read)."""
    listed = subprocess.run(["git", "-C", str(repo), "ls-files", "-z", "--", "*.rs"], capture_output=True, check=True)
    files = [f for f in listed.stdout.decode("utf-8").split("\0") if f]
    tracked = set(files)
    owners: set[str] = set()
    unresolved: list[str] = []
    for f in files:
        try:
            text = (repo / f).read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError) as why:
            unresolved.append(f"{f}: unreadable ({why.__class__.__name__})")
            continue
        crate_dir = posixpath.dirname(f)
        while crate_dir and not (repo / crate_dir / "Cargo.toml").exists():
            crate_dir = posixpath.dirname(crate_dir)
        try:
            loads, missed = indirect_loads(f, text, crate_dir)
        except Refusal as why:
            if "loads" in text or "include" in text or "path" in text:
                unresolved.append(f"{f}: not lexable ({why})")
            continue
        unresolved.extend(missed)
        owners.update(t for t in loads if t in tracked and t.startswith(SOURCE_ROOTS) and t != f)
    return sorted(owners), unresolved


INDIRECT_OWNERS_HEADER = (
    "# Published-source files another file loads through `#[path]` or `include!`.\n"
    "# Their `mod name;` children resolve from the loader, so the inline-cfg-test-v1\n"
    "# relocation (#15217) refuses them. Regenerate with\n"
    "# `python3 .github/actions/pr-candidate-admission/mechanical_relocation.py indirect-owners --write`.\n"
)


# --------------------------------------------------------------------------
# Self-test: every refusal branch has a discriminating fixture.


def self_test() -> None:
    owner = "crates/ripr/src/output/foo.rs"
    base_owner = (
        "pub fn shown() -> u32 {\n    1\n}\n\n"
        "#[cfg(test)]\nmod tests {\n    use super::*;\n\n    // keeps its comment\n"
        "    const RAW: &str = r#\"\n    keep\n        indent\n\"#;\n\n"
        '    #[test]\n    fn reads() {\n        assert_eq!(shown(), 1);\n        let _ = include_str!("fixture.txt");\n    }\n}\n'
    )
    target = "crates/ripr/src/output/foo/tests.rs"
    move = plan_moves(owner, base_owner)[0]
    head_owner = owner_after(base_owner, owner, {"tests"})
    assert head_owner.endswith("#[cfg(test)]\nmod tests;\n"), head_owner
    head_target = target_text(move)
    assert head_target.startswith("use super::*;\n"), head_target
    assert 'include_str!("../fixture.txt")' in head_target, head_target
    assert "\n    keep\n        indent\n" in head_target, "raw string contents keep their bytes"
    baseline = "# header\ncrates/ripr/src/a.rs|1\ncrates/ripr/src/output/foo.rs|1\n"
    trace = f'[x]\ntests = [\n  "{owner}::tests::reads",\n  "{owner}",\n]\n'

    def run(
        files: dict[str, tuple[str | None, str | None]],
        statuses: dict[str, str] | None = None,
        unchanged: dict[str, str] | None = None,
    ) -> Verdict:
        statuses = statuses or {}
        unchanged = unchanged or {}
        changed = []
        for path, (b, h) in files.items():
            status = statuses.get(path) or ("added" if b is None else "removed" if h is None else "modified")
            changed.append({"filename": path, "status": status})

        def read(path: str, side: str) -> str | None:
            if path in unchanged:
                return unchanged[path]
            if path == INDIRECT_OWNERS_PATH and path not in files:
                return INDIRECT_OWNERS_HEADER + "crates/ripr/src/output/loaded.rs\n"
            b, h = files.get(path, (None, None))
            return b if side == "base" else h

        for entry in changed:
            h = read(entry["filename"], "head")
            if h is not None:
                entry["sha"] = git_blob_sha(h)
        return verify(changed, read, require_blob_sha=True)

    good = {
        owner: (base_owner, head_owner),
        target: (None, head_target),
        BASELINE_PATH: (baseline, "# header\ncrates/ripr/src/a.rs|1\n"),
        ".ripr/traceability.toml": (trace, trace.replace(f"{owner}::tests::reads", f"{target}::reads")),
    }
    verdict = run(good)
    assert verdict.status == "validated", verdict.detail
    assert verdict.moves == [{"owner": owner, "module": "tests", "target": target, "lines": move.body_lines}]

    # rustfmt freedom: re-wrapped lines and a dropped trailing comma still match.
    rewrapped = head_target.replace("assert_eq!(shown(), 1);", "assert_eq!(\n        shown(),\n        1,\n    );")
    assert run({**good, target: (None, rewrapped)}).status == "validated"
    assert normalize(["|", "x", "|", "{", "S", "{", "a", "}", "}"]) == ["|", "x", "|", "S", "{", "a", "}"]
    # A closure block with statements keeps its braces.
    assert normalize(["|", "|", "{", "a", "(", ")", ";", "b", "}"]) == ["|", "|", "{", "a", "(", ")", ";", "b", "}"]

    def refused(
        files: dict[str, tuple[str | None, str | None]],
        needle: str,
        statuses: dict[str, str] | None = None,
        unchanged: dict[str, str] | None = None,
    ) -> None:
        v = run(files, statuses, unchanged)
        assert v.status == "rejected" and needle in v.detail, (needle, v.detail)

    refused({**good, target: (None, head_target.replace("1);", "2);"))}, "not the token-identical body")
    refused({**good, target: (None, head_target.replace("keep\n", "kept\n"))}, "not the token-identical body")
    refused({**good, target: (None, head_target.replace("// keeps its comment", "// edited"))}, "comments")
    # A comment may not reattach to another item: `// covered-by:` above `fn a`
    # moved above `fn b` keeps the same comment list but not its anchor.
    moved_comment = head_target.replace("// keeps its comment\n", "").replace(
        "#[test]\nfn reads", "// keeps its comment\n#[test]\nfn reads"
    )
    assert moved_comment != head_target, moved_comment
    refused({**good, target: (None, moved_comment)}, "comments")
    # Across an item boundary with no identifier between: the end of one body
    # and the attributes of the next item differ by the next code token.
    tail = "fn a() {\n    x();\n}\n\n// covered-by: b\n#[test]\nfn b() {}\n"
    into_a = "fn a() {\n    x();\n    // covered-by: b\n}\n\n#[test]\nfn b() {}\n"
    assert significant(tail) == significant(into_a)
    assert comment_texts(tail) != comment_texts(into_a)
    # Only Rust's whitespace separates tokens: NBSP is not whitespace to rustc.
    try:
        lex("let x\u00a0= 1;")
    except Refusal as why:
        assert "non-ASCII" in str(why), why
    else:
        raise AssertionError("NBSP was lexed as whitespace")
    # A non-ASCII identifier is one token, so whitespace cannot split it into
    # tokens that compare equal; other non-ASCII outside strings refuses.
    assert raw_tokens("let mutéx = 1;") == ["let", "mutéx", "=", "1", ";"]
    assert significant("fn f() { let mutéx = 1; }") != significant("fn f() { let mut é x = 1; }")
    try:
        lex("let x = 1 − 2;")
    except Refusal as why:
        assert "non-ASCII" in str(why), why
    else:
        raise AssertionError("a non-ASCII operator was lexed")
    assert len(lex('let s = "−"; // −')) > 0
    refused({**good, owner: (base_owner, head_owner.replace("1\n}", "2\n}", 1))}, "beyond the relocated module")
    refused({**good, "crates/ripr/src/other.rs": ("fn a() {}\n", "fn b() {}\n")}, "relocated no inline test module")
    refused({**good, "README.md": ("a\n", "b\n")}, "not an owner, a new test file or relocation metadata")
    refused({**good, "crates/ripr/src/output/extra.rs": (None, "fn a() {}\n")}, "are not relocation targets")
    refused({**good, target: ("pre-existing\n", head_target)}, "already exists", {target: "added"})
    refused({**good, BASELINE_PATH: (baseline, baseline)}, "one ceiling removed")
    refused({k: v for k, v in good.items() if k != BASELINE_PATH}, "must lower the ceilings", unchanged={BASELINE_PATH: baseline})
    refused(
        {**good, ".ripr/traceability.toml": (trace, trace.replace(f'"{owner}",', f'"{owner}",\n  "new",'))},
        "not the base re-keyed",
    )
    refused(
        {**good, ".ripr/traceability.toml": (trace, trace.replace("::tests::reads", "::tests::other"))},
        "not the base re-keyed",
    )
    # A bare owner reference (a production file) is never re-pointed.
    refused(
        {**good, ".ripr/traceability.toml": (trace, trace.replace(f"{owner}::tests::reads", f"{target}::reads").replace(f'"{owner}",', f'"{target}",'))},
        "not the base re-keyed",
    )
    # An allowlist row follows the move only when its pattern left the owner.
    allow = f"{owner}|include_str|1|fixture read\n"
    moved_row = allow.replace(owner, target)
    assert run({**good, "policy/process_allowlist.txt": (allow, moved_row)}).status == "validated"
    kept_owner = base_owner.replace("pub fn shown", "// include_str\npub fn shown")
    kept_head = owner_after(kept_owner, owner, {"tests"})
    refused(
        {**good, owner: (kept_owner, kept_head), "policy/process_allowlist.txt": (allow, moved_row)},
        "not the base re-keyed",
    )
    refused({**good, "policy/architecture.txt": ("a\n", "b\n")}, "not an owner, a new test file or relocation metadata")

    # Owners another file loads, and crate roots the transform does not model.
    loaded = "crates/ripr/src/output/loaded.rs"
    loaded_target = target_for(loaded, "tests")[0]
    loaded_files = {
        loaded: (base_owner, head_owner),
        loaded_target: (None, head_target),
        BASELINE_PATH: (baseline.replace(owner, loaded), "# header\ncrates/ripr/src/a.rs|1\n"),
    }
    refused(loaded_files, "is loaded through")
    refused(good, "missing at the base", unchanged={INDIRECT_OWNERS_PATH: None})  # type: ignore[dict-item]
    assert indirect_loads(
        "crates/ripr/src/a.rs",
        '#[cfg_attr(unix, path = "x.rs")]\nmod x;\ninclude!("y.rs");\nconst S: &str = include_str!("z.rs");\nlet path = p;\n',
        "crates/ripr",
    ) == ({"crates/ripr/src/x.rs", "crates/ripr/src/a/x.rs", "crates/ripr/src/y.rs"}, [])
    # Load forms the scan cannot resolve are reported, never skipped.
    for text, needle in (
        ("macro_rules! m { ($p:literal) => { #[path = $p] mod x; } }", "path = $"),
        ('#[path = "x\\u{2e}rs"]\nmod x;', "path = "),
        ('include!(concat!("a", "b.rs"));', "include!(concat"),
        ('include!(concat!(env!("CARGO_MANIFEST_DIR"), "/a", "/b.rs"));', "include!(concat"),
        ("macro_rules! vec { () => {} }", "shadows a comma-neutral macro"),
        ("pub(crate) use crate::real as assert_eq;", "may alias another macro"),
    ):
        _, missed = indirect_loads("crates/ripr/src/a.rs", text, "crates/ripr")
        assert len(missed) == 1 and needle in missed[0], (text, missed)
    # A shadowing definition outside the published roots does not reach them.
    assert indirect_loads("xtask/src/a.rs", "macro_rules! vec { () => {} }", "xtask") == (set(), [])

    # The listing must name the exact head blobs that were read.
    stale = verify(
        [{"filename": p, "status": "added" if b is None else "modified", "sha": "0" * 40} for p, (b, _) in good.items()],
        lambda path, side: (INDIRECT_OWNERS_HEADER if path == INDIRECT_OWNERS_PATH else
                            (good.get(path, (None, None))[0 if side == "base" else 1])),
        require_blob_sha=True,
    )
    assert stale.status == "rejected" and "listed blob" in stale.detail, stale.detail

    # Token identity keeps operator spelling, 1-tuples and macro arms.
    def body_refused(base_stmt: str, head_stmt: str) -> None:
        b = base_owner.replace("assert_eq!(shown(), 1);", base_stmt)
        moved = target_text(plan_moves(owner, b)[0])
        assert base_stmt.strip() in moved, base_stmt
        h_target = moved.replace(base_stmt.strip(), head_stmt)
        refused({**good, owner: (b, owner_after(b, owner, {"tests"})), target: (None, h_target)}, "not the token-identical body")

    body_refused("assert!(!v.is_empty() && one(&v));", "assert!(!v.is_empty() & &one(&v));")
    body_refused("let t = (1u8,);", "let t = (1u8);")
    body_refused("chk!(x);", "chk!(x,);")
    body_refused("let f = |x| { x + 1 }.max(2);", "let f = |x| x + 1.max(2);")
    assert normalize(["f", "(", "a", ",", ")"]) == ["f", "(", "a", ")"]
    assert normalize(["S", "<", "A", ",", ">"]) == ["S", "<", "A", ">"]
    assert normalize(["assert", "!", "(", "a", ",", ")"]) == ["assert", "!", "(", "a", ")"]
    assert normalize(["=", "(", "a", ",", ")"]) == ["=", "(", "a", ",", ")"]
    assert normalize(["m", "!", "(", "a", ",", "b", ",", ")"]) == ["m", "!", "(", "a", ",", "b", ",", ")"]
    assert normalize(["||", "{", "a", "}", ","]) == ["||", "a", ","]
    # Match-arm blocks: `=> { e } P =>` and `=> e, P =>` agree; `=> { e } }` and `=> e, }` too.
    arm = ["match", "x", "{", "A", "=>", "{", "f", "(", ")", "}", "B", "=>", "{", "g", "(", ")", "}", "}"]
    flat = ["match", "x", "{", "A", "=>", "f", "(", ")", ",", "B", "=>", "g", "(", ")", ",", "}"]
    assert normalize(arm) == normalize(flat), (normalize(arm), normalize(flat))
    # ...but a block arm whose next tokens would continue the expression does not.
    assert normalize(["=>", "{", "a", "}", "-", "1", "=>"]) != normalize(["=>", "a", "-", "1", "=>"])
    body_refused("match 1 { 1 => { one() } _ => two() }", "match 1 { 1 => one() _ => two() }")
    # Binary `|`/`||` before a block is not a closure: `a | { b == c }` keeps its braces.
    assert normalize(["a", "|", "{", "b", "==", "c", "}", ","]) == ["a", "|", "{", "b", "==", "c", "}", ","]
    assert normalize(["f", "(", ")", "||", "{", "b", "}", ")"]) == ["f", "(", ")", "||", "{", "b", "}", ")"]
    body_refused("let v = a | { b == c };", "let v = a | b == c;")
    # Real closures after `move`, `(` or `=` still drop the block.
    assert normalize(["move", "|", "x", "|", "{", "e", "}", ")"]) == ["move", "|", "x", "|", "e", ")"]
    assert normalize(["(", "|", "x", ",", "y", "|", "{", "e", "}", ")"]) == ["(", "|", "x", ",", "y", "|", "e", ")"]
    # A block-like first expression keeps the braces (closure and match arm).
    if_else = ["if", "c", "{", "1", "}", "else", "{", "2", "}"]
    assert normalize(["=", "|", "c", "|", "{", *if_else, "}", ";"]) == ["=", "|", "c", "|", "{", *if_else, "}", ";"]
    assert normalize(["=>", "{", *if_else, "}", ","]) == ["=>", "{", *if_else, "}", ","]
    body_refused("let f = |c: bool| { if c { 1 } else { 2 } };", "let f = |c: bool| if c { 1 } else { 2 };")
    # A closing bar after a typed parameter is not an opener: `|x: Vec<u8>| y | { b == c }`.
    typed = significant("fn f() { let g = |x: Vec<u8>| y | { b == c }; }")
    assert typed != significant("fn f() { let g = |x: Vec<u8>| y | b == c; }"), typed
    assert significant("fn f() { let g = |x: Vec<u8>| { x.len() }; }") == significant("fn f() { let g = |x: Vec<u8>| x.len(); }")
    # A leading macro call or attribute keeps the braces: `m!{x} - 1` is a statement then `-1`.
    assert significant("fn f() { let g = |c| { m!{x} - 1 }; }") != significant("fn f() { let g = |c| m!{x} - 1; }")
    assert significant("fn f() { g(&|_, _| { true }); }") == significant("fn f() { g(&|_, _| true); }")
    assert significant("fn f() { let g = |c| { m!(x) - 1 }; }") == significant("fn f() { let g = |c| m!(x) - 1; }")
    assert significant("fn f() { let g = |c| { #[a] if c { 1 } else { 2 } }; }") != significant("fn f() { let g = |c| #[a] if c { 1 } else { 2 }; }")
    # An attribute's `]` does not make the next `(..)` a call: `#[a] (x,)` stays a 1-tuple.
    attr_tuple = ["#", "[", "a", "]", "(", "x", ",", ")"]
    assert normalize(attr_tuple) == attr_tuple
    # A doc comment documents the item after it; it cannot move to another item.
    assert significant("/// a\nfn f() {}\nfn g() {}\n") != significant("fn f() {}\n/// a\nfn g() {}\n")
    assert significant("/// a\nfn f() {}\n") == significant("    /// a\n    fn f() {}\n")
    assert significant("// a\nfn f() {}\nfn g() {}\n") == significant("fn f() {}\n// a\nfn g() {}\n")
    body_refused("/** one */ fn x() {} fn y() {}", "fn x() {} /** one */ fn y() {}")
    refused({**good, owner: (base_owner, base_owner + "\n")}, "relocated no inline test module")

    # A gate that compiles outside tests is not relocated; nothing moves.
    any_gate = base_owner.replace("#[cfg(test)]", '#[cfg(any(test, feature = "x"))]')
    assert top_level_test_modules(any_gate) == []
    assert top_level_test_modules(base_owner.replace("#[cfg(test)]", "#[cfg(not(test))]")) == []
    assert len(top_level_test_modules(base_owner.replace("#[cfg(test)]", "#[cfg(all(unix, test))]"))) == 1
    # A `)` inside a string literal is not structure: this gate compiles on Linux.
    assert not is_test_only_cfg('#[cfg(all(unix, any(target_os = ")", test, target_os = "linux")))]')
    assert not is_test_only_cfg("#[cfg(all(unix), test(x))]")
    assert is_test_only_cfg('#[cfg(all(target_os = ",)", test))]')
    # Spelling inside strings is data.
    assert top_level_test_modules('const S: &str = "#[cfg(test)]\\nmod tests {}";\n') == []

    # Constructs whose meaning depends on the file's location refuse.
    for construct, needle in (
        ("let _ = file!();", "`file!`"),
        ("let _ = line!();", "`line!`"),
        ('#[path = "x.rs"]\n    mod x;', "`path` attribute"),
        ('#[cfg_attr(test, path = "x.rs")]\n    mod x;', "`path` attribute"),
        ('#[r#path = "x.rs"]\n    mod x;', "`path` attribute"),
        ('#[cfg_attr(test, r#path = "x.rs")]\n    mod x;', "`path` attribute"),
        ("let _ = r#line!();", "`r#line!`"),
        ("use std::line as here;", "imports ['line']"),
        ("use std::{column, vec};", "imports ['column', 'vec']"),
        ("let _ = std::panic::Location::caller();", "reports the source location"),
        ("#[track_caller]\n    fn h() {}", "reports the source location"),
        ("let _ = stringify!(a + b);", "`stringify!`"),
        ('let _ = r#include_str!("x");', "does not re-base"),
        ("macro_rules! vec { () => {} }", "shadowing a macro"),
        ('let _ = include_str!["x"];', "does not re-base"),
        ('let _ = include_bytes!{"x"};', "does not re-base"),
    ):
        text = base_owner.replace("use super::*;", "use super::*;\n    " + construct)
        try:
            expected_body_tokens(plan_moves(owner, text)[0])
        except Refusal as why:
            assert needle in str(why), (needle, why)
        else:
            raise AssertionError(f"{construct} was not refused")

    # The owner: a shadowing import anywhere, or a location macro after the first
    # relocated module (every later line moves up), refuses; one before it does not.
    for prefix, suffix, needle in (
        ("use std::assert_eq as ok;\n", "", "imports ['assert_eq']"),
        ("", "\npub fn g() -> u32 {\n    line!()\n}\n", "after its first test module"),
        ("", "\n#[track_caller]\npub fn g() {}\n", "after its first test module"),
    ):
        try:
            plan_moves(owner, prefix + base_owner + suffix)
        except Refusal as why:
            assert needle in str(why), (needle, why)
        else:
            raise AssertionError(f"owner {prefix!r} {suffix!r} was not refused")
    assert plan_moves(owner, "pub fn g() -> u32 {\n    line!()\n}\n" + base_owner)
    # A relocated module that carries its own `path` attribute refuses.
    for attr in ('#[path = "t.rs"]', '#[cfg_attr(test, path = "t.rs")]', '#[r#path = "t.rs"]', '#[cfg_attr(test, r#path = "t.rs")]'):
        try:
            plan_moves(owner, base_owner.replace("#[cfg(test)]\n", "#[cfg(test)]\n" + attr + "\n"))
        except Refusal as why:
            assert "carries a `path` attribute" in str(why), why
        else:
            raise AssertionError(f"{attr} on the module was not refused")

    # A module whose name is not a test name gets a `#[path]` to a `_tests.rs`.
    named = base_owner.replace("mod tests {", "mod harness_verdict {")
    named_move = plan_moves(owner, named)[0]
    assert named_move.target == "crates/ripr/src/output/foo/harness_verdict_tests.rs", named_move.target
    named_owner = owner_after(named, owner, {"harness_verdict"})
    assert '#[cfg(test)]\n#[path = "foo/harness_verdict_tests.rs"]\nmod harness_verdict;\n' in named_owner, named_owner
    # The named target is loaded through the transform's `#[path]`, so the
    # indirect-owner list gains exactly that target, and only then.
    indirect_base = INDIRECT_OWNERS_HEADER + "crates/ripr/src/output/loaded.rs\n"
    indirect_head = indirect_after(indirect_base, [named_move])
    assert indirect_head == INDIRECT_OWNERS_HEADER + named_move.target + "\ncrates/ripr/src/output/loaded.rs\n", indirect_head
    named_good = {
        owner: (named, named_owner),
        named_move.target: (None, target_text(named_move)),
        BASELINE_PATH: (baseline, "# header\ncrates/ripr/src/a.rs|1\n"),
        INDIRECT_OWNERS_PATH: (indirect_base, indirect_head),
    }
    assert run(named_good).status == "validated", run(named_good).detail
    refused({k: v for k, v in named_good.items() if k != INDIRECT_OWNERS_PATH}, "must list the targets")
    refused({**named_good, INDIRECT_OWNERS_PATH: (indirect_base, indirect_head + "crates/ripr/src/x.rs\n")}, "not the base plus")
    refused({**good, INDIRECT_OWNERS_PATH: (indirect_base, indirect_head)}, "not the base plus")
    # A test-intent block (`test` and `path` keys) follows its moved test only.
    intent = f'[[test_intent]]\ntest = "reads"\npath = "{owner}"\nintent = "smoke"\n\n[[test_intent]]\ntest = "shown"\npath = "{owner}"\n'
    intent_after = rewrite_metadata(intent, [move], {owner: head_owner})
    assert intent_after == intent.replace(f'path = "{owner}"', f'path = "{target}"', 1), intent_after
    assert run({**good, ".ripr/test_intent.toml": (intent, intent_after)}).status == "validated"
    refused(
        {**good, ".ripr/test_intent.toml": (intent, intent.replace(f'path = "{owner}"', f'path = "{target}"'))},
        "not the base re-keyed",
    )
    # mod.rs and crate-root owners resolve beside themselves; a nested lib.rs does not.
    assert target_for("crates/ripr/src/lsp/mod.rs", "tests") == ("crates/ripr/src/lsp/tests.rs", None)
    assert target_for("crates/ripr/src/lib.rs", "tests") == ("crates/ripr/src/tests.rs", None)
    assert target_for("crates/ripr/src/a/lib.rs", "tests") == ("crates/ripr/src/a/lib/tests.rs", None)
    # Out of scope and declaration parsing.
    try:
        plan_moves("xtask/src/main.rs", base_owner)
    except Refusal:
        pass
    else:
        raise AssertionError("xtask owner was not refused")
    assert declared("x\nMechanical relocation: `inline-cfg-test-v1`\n") == TRANSFORM_ID
    assert declared("no line") is None
    assert declared("Mechanical relocation: a\nMechanical relocation: b\n") == ""
    print("mechanical relocation self-test: pass")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("self-test")
    p_apply = sub.add_parser("apply", help="relocate every top-level inline test module of each owner")
    p_apply.add_argument("--repo", type=Path, default=Path("."))
    p_apply.add_argument("--no-rustfmt", action="store_true")
    p_apply.add_argument("owners", nargs="+")
    p_indirect = sub.add_parser("indirect-owners", help=f"print, --check or --write {INDIRECT_OWNERS_PATH}")
    p_indirect.add_argument("--repo", type=Path, default=Path("."))
    p_indirect.add_argument("--check", action="store_true")
    p_indirect.add_argument("--write", action="store_true")
    p_verify = sub.add_parser("verify", help="verify base..head (head may be WORKTREE)")
    p_verify.add_argument("--repo", type=Path, default=Path("."))
    p_verify.add_argument("--base", required=True)
    p_verify.add_argument("--head", default="WORKTREE")
    args = parser.parse_args()
    if args.command == "self-test":
        self_test()
        return 0
    if args.command == "indirect-owners":
        owners, unresolved = indirect_owners(args.repo)
        if unresolved:
            print("indirect-owners: load forms the scan cannot resolve (use one plain literal):", file=sys.stderr)
            for u in unresolved:
                print(f"  {u}", file=sys.stderr)
            return 1
        expected = INDIRECT_OWNERS_HEADER + "".join(f"{o}\n" for o in owners)
        path = args.repo / INDIRECT_OWNERS_PATH
        if args.write:
            path.write_text(expected, encoding="utf-8")
        elif args.check:
            current = path.read_text(encoding="utf-8") if path.exists() else ""
            if current != expected:
                print(f"{INDIRECT_OWNERS_PATH} is stale; regenerate with indirect-owners --write", file=sys.stderr)
                return 1
            print(f"{INDIRECT_OWNERS_PATH}: current")
        else:
            sys.stdout.write(expected)
        return 0
    if args.command == "apply":
        try:
            moves = apply(args.repo, args.owners, rustfmt=not args.no_rustfmt)
        except Refusal as why:
            print(json.dumps({"status": "refused", "detail": str(why)}))
            return 1
        print(json.dumps([{"owner": m.owner, "module": m.name, "target": m.target, "lines": m.body_lines} for m in moves]))
        return 0
    verdict = verify(git_changed(args.repo, args.base, args.head), git_reader(args.repo, args.base, args.head))
    print(json.dumps(verdict.record(), sort_keys=True))
    return 0 if verdict.status == "validated" else 1


if __name__ == "__main__":
    sys.exit(main())
