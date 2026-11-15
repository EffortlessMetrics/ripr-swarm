//! #14760: an identifier renamed at its declaration inside one diff.
//!
//! A changed line that differs from its old line only by a name the same
//! file's diff renames is not a behavior edit: `HEADER_NAME_MAP[b as usize]`
//! -> `TOKEN_MAP[b as usize]` reads the same table. The rename counts only
//! when the diff proves it in this file:
//!
//! - one `static`, `const`, `fn` or `type` declaration line changes only its
//!   name, and that item has no attribute that could observe the name;
//! - the whole item, after the name, is byte-identical on both sides;
//! - the old name no longer appears in the new file's code, and the new name
//!   did not appear in the old file's code;
//! - neither name is spelled after `.` or `::`, as a field, inside a macro
//!   invocation, or inside a `struct`/`enum`/`union` body, where the same
//!   spelling may name something else;
//! - neither file can observe identifier text (`stringify!`, `type_name`).
//!
//! Anything else (a body edit, a new name that already existed, a partial
//! rename, a use site that also changes another token) stays probed.

use crate::analysis::diff::ChangedFile;
use crate::analysis::extract::mask_comments_and_strings;
use std::collections::{BTreeMap, BTreeSet};

/// Declaration keywords whose renamed name no runtime path can observe. A
/// `struct` or `enum` name is excluded: a derived `Debug` prints it.
const RENAMEABLE_ITEMS: [&str; 4] = ["static", "const", "fn", "type"];

/// Names the toolchain binds by spelling: renaming one changes what runs.
const ENTRY_POINTS: [&str; 1] = ["main"];

/// Attributes that never read an item's name. Any other attribute directly
/// above the declaration (`#[no_mangle]`, `#[export_name]`, `#[test]`, a
/// tracing `#[instrument]`) keeps the rename probed.
const NAME_BLIND_ATTRIBUTES: [&str; 10] = [
    "inline", "must_use", "allow", "expect", "warn", "deny", "cfg", "doc", "cold", "rustfmt",
];

/// Identifier prefixes whose presence anywhere in a file means identifier
/// spelling can reach runtime output (`type_name_of_val` too), so no rename
/// in that file is established.
const NAME_OBSERVERS: [&str; 5] = [
    "stringify",
    "concat_idents",
    "type_name",
    "paste",
    "function_name",
];

/// Items whose bodies hold fields or variants: a name spelled there is a
/// field or variant, not the renamed item.
const TYPE_BODY_ITEMS: [&str; 3] = ["struct", "enum", "union"];

/// Lines of one changed file that only apply a rename the diff proves: the
/// new-side lines of the added half and the old-side lines of the removed
/// half of each positional replacement pair.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct RenameOnlyLines {
    pub(super) added: BTreeSet<usize>,
    pub(super) removed: BTreeSet<usize>,
}

pub(super) fn rename_only_lines(
    changed: &ChangedFile,
    new_source: &str,
    old_source: &str,
) -> RenameOnlyLines {
    let new = SourceView::new(new_source);
    let old = SourceView::new(old_source);
    let mut result = RenameOnlyLines::default();
    if new.observes_names() || old.observes_names() {
        return result;
    }
    let pairs = changed
        .replacement_counterparts()
        .into_iter()
        .filter_map(|(new_line, removed)| {
            let renames = identifier_substitutions(&new, new_line, &old, removed.line)?;
            Some((new_line, removed.line, renames))
        })
        .collect::<Vec<_>>();
    let established = established_renames(&pairs, &new, &old);
    if established.is_empty() {
        return result;
    }
    for (new_line, old_line, renames) in &pairs {
        if renames
            .iter()
            .all(|(from, to)| established.get(from.as_str()) == Some(&to.as_str()))
        {
            result.added.insert(*new_line);
            result.removed.insert(*old_line);
        }
    }
    result
}

/// A positional replacement pair (new-side line, old-side line) and the
/// identifier substitutions that are its only difference.
type ReplacementRenames = (usize, usize, Vec<(String, String)>);

/// One side of the file: raw text and the comment/string mask with the same
/// byte offsets.
struct SourceView<'a> {
    raw: &'a str,
    masked: String,
    /// Byte offset where each 1-based line starts (`line_starts[0]` is line 1).
    line_starts: Vec<usize>,
}

impl<'a> SourceView<'a> {
    fn new(raw: &'a str) -> Self {
        let masked = mask_comments_and_strings(raw);
        let mut line_starts = vec![0];
        line_starts.extend(raw.match_indices('\n').map(|(at, _)| at + 1));
        Self {
            raw,
            masked,
            line_starts,
        }
    }

    /// Byte range of 1-based `line` without its line ending.
    fn line_range(&self, line: usize) -> Option<(usize, usize)> {
        let start = *self.line_starts.get(line.checked_sub(1)?)?;
        let end = self
            .line_starts
            .get(line)
            .map_or(self.raw.len(), |next| next - 1);
        let end = if self.raw[start..end].ends_with('\r') {
            end - 1
        } else {
            end
        };
        (start <= end && self.masked.len() == self.raw.len()).then_some((start, end))
    }

    fn observes_names(&self) -> bool {
        identifiers(&self.masked, 0, self.masked.len()).any(|(_, name)| {
            NAME_OBSERVERS
                .iter()
                .any(|observer| name.starts_with(observer))
        })
    }

    /// Whether `name` is spelled where it may name something other than the
    /// renamed item, or where its text can be observed:
    ///
    /// - a field (`name: T`, `S { name: v }`; a `static`/`const`
    ///   declaration's own `name: T` is not a field);
    /// - after `.` (`v.name()` may be a receiver's unrelated method) or `::`
    ///   (a path may resolve to another item);
    /// - inside any macro invocation or `macro_rules!` body: a macro sees
    ///   tokens, not resolved names, and may match or print them;
    /// - inside a `struct`, `enum` or `union` body, where a same-spelled
    ///   field or variant reaches derived `Debug`/serde output.
    fn spells_observably(&self, name: &str) -> bool {
        let masked = self.masked.as_str();
        let mut opaque_ends = Vec::new();
        let mut previous: Option<(usize, &str)> = None;
        for (at, found) in identifiers(masked, 0, masked.len()) {
            let declared = previous.is_some_and(|(end, word)| {
                matches!(word, "static" | "const" | "mut") && masked[end..at].trim().is_empty()
            });
            previous = Some((at + found.len(), found));
            let after = masked[at + found.len()..].trim_start();
            if after.starts_with('!') && !after.starts_with("!=") {
                opaque_ends.push(invocation_end(masked, at + found.len()));
            }
            if TYPE_BODY_ITEMS.contains(&found) {
                opaque_ends.push(type_body_end(masked, at + found.len()));
            }
            if found != name || declared {
                continue;
            }
            let opaque = opaque_ends.iter().any(|end| end.is_none_or(|end| at < end));
            let field_type = after.starts_with(':') && !after.starts_with("::");
            let before = masked[..at].trim_end();
            let member = before.ends_with('.') && !before.ends_with("..");
            let qualified = before.ends_with("::") || after.starts_with("::");
            if opaque || field_type || member || qualified {
                return true;
            }
        }
        false
    }

    fn mentions(&self, name: &str) -> bool {
        identifiers(&self.masked, 0, self.masked.len()).any(|(_, found)| found == name)
    }
}

/// Byte offset just past the end of the `struct`/`enum`/`union` whose keyword
/// ends at `from`: its `;` or the `}` closing its body. `None` when it never
/// ends.
fn type_body_end(masked: &str, from: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, ch) in masked[from..].char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' => depth = depth.checked_sub(1)?,
            '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(from + offset + 1);
                }
            }
            ';' if depth == 0 => return Some(from + offset + 1),
            _ => {}
        }
    }
    None
}

/// Byte offset just past the delimiter that closes the macro invocation whose
/// `!` follows `from`. `None` when it never closes (the rest of the file then
/// counts as inside it).
fn invocation_end(masked: &str, from: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, ch) in masked[from..].char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(from + offset + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// Code identifiers in `masked[start..end]`, as (byte offset, text). Numeric
/// literals (`256usize`) are skipped whole so a suffix is not a name.
fn identifiers(masked: &str, start: usize, end: usize) -> impl Iterator<Item = (usize, &str)> {
    let text = &masked[start..end];
    let mut chars = text.char_indices().peekable();
    std::iter::from_fn(move || {
        while let Some((at, ch)) = chars.next() {
            if ch.is_ascii_digit() || continues_identifier(ch) {
                let mut stop = at + ch.len_utf8();
                while let Some(&(next_at, next)) = chars.peek() {
                    if !continues_identifier(next) {
                        break;
                    }
                    stop = next_at + next.len_utf8();
                    chars.next();
                }
                if !ch.is_ascii_digit() {
                    return Some((start + at, &text[at..stop]));
                }
            }
        }
        None
    })
}

fn continues_identifier(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || (!ch.is_ascii() && !ch.is_whitespace())
}

/// The identifier substitutions that turn old line `old_line` into new line
/// `new_line`, when they are the only difference: both lines have the same
/// identifiers in order, and replacing each differing one in the old raw text
/// yields the new raw text exactly. `None` when anything else differs or no
/// identifier does. A name must map to one replacement throughout.
fn identifier_substitutions(
    new: &SourceView<'_>,
    new_line: usize,
    old: &SourceView<'_>,
    old_line: usize,
) -> Option<Vec<(String, String)>> {
    let (new_start, new_end) = new.line_range(new_line)?;
    let (old_start, old_end) = old.line_range(old_line)?;
    let new_names = identifiers(&new.masked, new_start, new_end).collect::<Vec<_>>();
    let old_names = identifiers(&old.masked, old_start, old_end).collect::<Vec<_>>();
    if new_names.len() != old_names.len() {
        return None;
    }
    let mut rebuilt = String::new();
    let mut cursor = old_start;
    let mut renames = BTreeMap::<&str, &str>::new();
    for ((_, to), (at, from)) in new_names.iter().zip(&old_names) {
        if from == to {
            continue;
        }
        if *renames.entry(from).or_insert(to) != *to {
            return None;
        }
        rebuilt.push_str(&old.raw[cursor..*at]);
        rebuilt.push_str(to);
        cursor = at + from.len();
    }
    rebuilt.push_str(&old.raw[cursor..old_end]);
    (!renames.is_empty() && rebuilt == new.raw[new_start..new_end]).then(|| {
        renames
            .into_iter()
            .map(|(from, to)| (from.to_owned(), to.to_owned()))
            .collect()
    })
}

/// Old name -> new name for every rename the diff proves (see the module
/// docs). A name declared by more than one changed declaration line is not
/// established: which item it means is ambiguous.
fn established_renames<'a>(
    pairs: &'a [ReplacementRenames],
    new: &SourceView<'_>,
    old: &SourceView<'_>,
) -> BTreeMap<&'a str, &'a str> {
    let mut declarations = BTreeMap::<&str, Vec<(&str, usize, usize)>>::new();
    for (new_line, old_line, renames) in pairs {
        let [(from, to)] = renames.as_slice() else {
            continue;
        };
        if declares(new, *new_line, to) && declares(old, *old_line, from) {
            declarations.entry(from.as_str()).or_default().push((
                to.as_str(),
                *new_line,
                *old_line,
            ));
        }
    }
    declarations
        .into_iter()
        .filter_map(|(from, found)| {
            let [(to, new_line, old_line)] = found.as_slice() else {
                return None;
            };
            let established = !ENTRY_POINTS.contains(&from)
                && !ENTRY_POINTS.contains(to)
                && !new.mentions(from)
                && !old.mentions(to)
                && !old.spells_observably(from)
                && !new.spells_observably(to)
                && name_blind_attributes(new, *new_line)
                && name_blind_attributes(old, *old_line)
                && same_item_after_rename(new, *new_line, old, *old_line, from, to);
            established.then_some((from, *to))
        })
        .collect()
}

/// Whether 1-based `line` declares `name` as a [`RENAMEABLE_ITEMS`] item:
/// the keyword (`static mut` too) directly precedes the name, with only
/// visibility, `pub(..)`, `unsafe`, `async`, `extern` or `const` qualifiers
/// before it.
fn declares(view: &SourceView<'_>, line: usize, name: &str) -> bool {
    let Some((start, end)) = view.line_range(line) else {
        return false;
    };
    let words = identifiers(&view.masked, start, end).collect::<Vec<_>>();
    let Some(at) = words.iter().position(|(_, word)| *word == name) else {
        return false;
    };
    let before = &words[..at];
    let keyword = match before {
        [.., (_, "static"), (_, "mut")] => "static",
        [.., (_, keyword)] => keyword,
        [] => return false,
    };
    let keyword_at = before
        .iter()
        .rposition(|(_, word)| *word == keyword)
        .unwrap_or(0);
    RENAMEABLE_ITEMS.contains(&keyword)
        && before[..keyword_at].iter().all(|(_, word)| {
            matches!(
                *word,
                "pub" | "crate" | "super" | "self" | "in" | "unsafe" | "async" | "extern" | "const"
            )
        })
        && view.masked[start..words[at].0]
            .chars()
            .all(|ch| ch.is_alphanumeric() || ch.is_whitespace() || "_()".contains(ch))
}

/// Whether every attribute line directly above 1-based `line` (doc comments
/// are masked to blank) is a [`NAME_BLIND_ATTRIBUTES`] attribute.
fn name_blind_attributes(view: &SourceView<'_>, line: usize) -> bool {
    let mut above = line;
    while above > 1 {
        above -= 1;
        let Some((start, end)) = view.line_range(above) else {
            return false;
        };
        let text = view.masked[start..end].trim();
        // A blank line or masked doc comment may sit between an attribute
        // and its item (`#[no_mangle]`, blank, `static OLD`).
        if text.is_empty() {
            continue;
        }
        let Some(attribute) = text.strip_prefix("#[") else {
            // The tail of a multi-line attribute hides which attribute it
            // is; anything else ends the attribute run.
            return !text.ends_with(']') && !text.ends_with(')');
        };
        let name = attribute
            .split(|ch: char| !continues_identifier(ch))
            .next()
            .unwrap_or_default();
        if !NAME_BLIND_ATTRIBUTES.contains(&name) {
            return false;
        }
    }
    true
}

/// Whether the item declared on `new_line` is the item declared on
/// `old_line` with `from` respelled to `to` throughout, byte for byte.
fn same_item_after_rename(
    new: &SourceView<'_>,
    new_line: usize,
    old: &SourceView<'_>,
    old_line: usize,
    from: &str,
    to: &str,
) -> bool {
    let brace_ends = |view: &SourceView<'_>, line: usize, name: &str| {
        view.line_range(line).is_some_and(|(start, end)| {
            identifiers(&view.masked, start, end)
                .take_while(|(_, word)| *word != name)
                .any(|(_, word)| word == "fn")
        })
    };
    let (Some(new_item), Some(old_item)) = (
        item_range(new, new_line, brace_ends(new, new_line, to)),
        item_range(old, old_line, brace_ends(old, old_line, from)),
    ) else {
        return false;
    };
    let mut rebuilt = String::new();
    let mut cursor = old_item.0;
    for (at, name) in identifiers(&old.masked, old_item.0, old_item.1) {
        if name == from {
            rebuilt.push_str(&old.raw[cursor..at]);
            rebuilt.push_str(to);
            cursor = at + name.len();
        }
    }
    rebuilt.push_str(&old.raw[cursor..old_item.1]);
    rebuilt == new.raw[new_item.0..new_item.1]
}

/// Byte range of the item that starts on 1-based `line`: through the first
/// `;` outside any bracket, or, for a `fn` (`brace_ends`), through the `}`
/// that closes its body. A `static`/`const` initializer may hold a block
/// (`= { 1 } + 2;`), so only its `;` ends it. `None` when the item never
/// ends.
fn item_range(view: &SourceView<'_>, line: usize, brace_ends: bool) -> Option<(usize, usize)> {
    let (start, _) = view.line_range(line)?;
    let mut depth = 0usize;
    for (offset, ch) in view.masked[start..].char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' => depth = depth.checked_sub(1)?,
            '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 && brace_ends {
                    return Some((start, start + offset + 1));
                }
            }
            ';' if depth == 0 => return Some((start, start + offset + 1)),
            _ => {}
        }
    }
    None
}
