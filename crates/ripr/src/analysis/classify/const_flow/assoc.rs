//! RIPR-SPEC-0048, associated-constant extension: bind an edited
//! `const NAME: T = ..;` inside `impl Trait for Type { .. }` to the trait
//! default methods that read it as `Self::NAME`.
//!
//! The binding is deliberately narrow. Anything the lexical scan cannot pin to
//! exactly one declaration, one impl and one reader name ends in `None`, and
//! the caller keeps `static_unknown`.

use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::facts::{FunctionContainer, FunctionSummary};

use super::super::const_readers::function_reads;

/// Trait default methods tried per edited constant.
const READER_LIMIT: usize = 6;

/// The impl that declares the edited constant and the trait default methods
/// that read it.
pub(in crate::analysis) struct AssocBinding<'a> {
    pub self_type: String,
    pub readers: Vec<&'a FunctionSummary>,
}

/// One brace block of the masked source: the words before its `{` and the
/// lines it spans.
struct Block {
    words: Vec<String>,
    start: usize,
    end: usize,
}

fn blocks(masked: &str) -> Vec<Block> {
    let mut done: Vec<Block> = Vec::new();
    let mut open: Vec<Block> = Vec::new();
    let mut header = String::new();
    let mut line = 1usize;
    for ch in masked.chars() {
        match ch {
            '{' => {
                open.push(Block {
                    words: header.split_whitespace().map(str::to_string).collect(),
                    start: line,
                    end: line,
                });
                header.clear();
            }
            '}' => {
                if let Some(mut block) = open.pop() {
                    block.end = line;
                    done.push(block);
                }
                header.clear();
            }
            ';' => header.clear(),
            '\n' => {
                line += 1;
                header.push(' ');
            }
            other => header.push(other),
        }
    }
    done
}

fn plain_ident(word: &str) -> bool {
    !word.is_empty()
        && word
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// `[pub] mod name` only: a module chain, no other enclosing block.
fn is_module(words: &[String]) -> bool {
    matches!(words, [m, name] if m == "mod" && plain_ident(name))
        || matches!(words, [p, m, name] if p == "pub" && m == "mod" && plain_ident(name))
}

/// `impl Trait for Type`, plain names, no generics, no attribute.
fn impl_header(words: &[String]) -> Option<(&str, &str)> {
    match words {
        [imp, tr, kw, ty] if imp == "impl" && kw == "for" && plain_ident(tr) && plain_ident(ty) => {
            Some((tr, ty))
        }
        _ => None,
    }
}

/// `[pub] trait Name`, no generics, no attribute.
fn trait_header(words: &[String]) -> Option<&str> {
    match words {
        [tr, name] if tr == "trait" && plain_ident(name) => Some(name),
        [p, tr, name] if p == "pub" && tr == "trait" && plain_ident(name) => Some(name),
        _ => None,
    }
}

/// Bind the constant `name` declared at `declaration_line` of `source`.
///
/// Holds only when: the declaration sits directly in one `impl Trait for Type`
/// (inside nothing but `mod` blocks); that is the only such impl in the file;
/// the file's one `trait Trait` declares `name` as a required constant in the
/// same module chain; and at least one default method of that trait with no
/// receiver reads `name`.
pub(in crate::analysis) fn associated_const_binding<'a>(
    source: &str,
    declaration_line: usize,
    name: &str,
    functions: impl IntoIterator<Item = &'a FunctionSummary>,
) -> Option<AssocBinding<'a>> {
    let masked = mask_comments_and_strings(source);
    let all = blocks(&masked);
    let mut enclosing: Vec<&Block> = all
        .iter()
        .filter(|block| block.start <= declaration_line && declaration_line <= block.end)
        .collect();
    enclosing.sort_by_key(|block| block.start);
    let (innermost, parents) = enclosing.split_last()?;
    let (trait_name, self_type) = impl_header(&innermost.words)?;
    if !parents.iter().all(|block| is_module(&block.words)) {
        return None;
    }
    let chain = |block: &Block| -> Vec<Vec<String>> {
        all.iter()
            .filter(|outer| {
                outer.start < block.start && block.end <= outer.end && !std::ptr::eq(*outer, block)
            })
            .map(|outer| outer.words.clone())
            .collect()
    };
    let impl_chain = chain(innermost);
    // The only `impl Trait for Type` of the file.
    let impls = all
        .iter()
        .filter(|block| impl_header(&block.words) == Some((trait_name, self_type)))
        .count();
    if impls != 1 {
        return None;
    }
    // The only trait of that name, in the same module chain.
    let traits: Vec<&Block> = all
        .iter()
        .filter(|block| trait_header(&block.words) == Some(trait_name))
        .collect();
    let [trait_block] = traits.as_slice() else {
        return None;
    };
    if chain(trait_block) != impl_chain {
        return None;
    }
    // The trait declares the constant and gives it no default.
    let declared: Vec<&str> = masked
        .lines()
        .enumerate()
        .filter(|(at, _)| (trait_block.start..=trait_block.end).contains(&(at + 1)))
        .map(|(_, line)| line.trim())
        .filter(|line| {
            let rest = line.strip_prefix("const ").map(str::trim_start);
            rest.and_then(|rest| rest.strip_prefix(name))
                .is_some_and(|after| after.trim_start().starts_with(':'))
        })
        .collect();
    if !matches!(declared.as_slice(), [only] if !only.contains('=')) {
        return None;
    }
    let readers: Vec<&FunctionSummary> = functions
        .into_iter()
        .filter(|function| {
            matches!(&function.item.container,
                FunctionContainer::Trait { trait_name: owner } if owner == trait_name)
                && function.item.has_body
                && !function.item.has_self_param
                && (trait_block.start..=trait_block.end).contains(&function.start_line)
                && function_reads(&function.body, name)
        })
        .take(READER_LIMIT)
        .collect();
    (!readers.is_empty()).then(|| AssocBinding {
        self_type: self_type.to_string(),
        readers,
    })
}
