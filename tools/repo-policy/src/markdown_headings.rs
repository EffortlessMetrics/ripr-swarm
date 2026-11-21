//! Canonical anchors for both Markdown gates.
use std::collections::{BTreeMap, BTreeSet};

/// The anchor GitHub gives a heading whose text is `text`.
///
/// GitHub lowercases the text, turns each space into `-`, and drops every other
/// character that is not a letter, digit, `_` or `-`. Dropping rather than
/// collapsing is load-bearing: an em-dash or a slash surrounded by spaces
/// leaves both of those spaces behind and so produces a doubled dash, which
/// links into this repository's own headings depend on.
pub fn heading_slug(text: &str) -> String {
    let mut slug = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == '-' {
            slug.extend(ch.to_lowercase());
        } else if ch == ' ' {
            slug.push('-');
        }
    }
    slug
}

/// Every anchor the headings of `text` offer.
///
/// Headings that slugify the same way are numbered the way GitHub numbers them:
/// the first keeps the bare slug and later ones gain `-1`, `-2`, and so on.
pub fn heading_slugs(text: &str) -> BTreeSet<String> {
    let mut slugs = BTreeSet::new();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut in_fence = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let Some(title) = atx_heading_title(trimmed) else {
            continue;
        };
        let base = heading_slug(title);
        let count = seen.entry(base.clone()).or_insert(0);
        let slug = if *count == 0 {
            base
        } else {
            format!("{base}-{count}")
        };
        *count += 1;
        slugs.insert(slug);
    }
    slugs
}

/// The heading text of an ATX heading line, or `None` when `trimmed` is not one.
fn atx_heading_title(trimmed: &str) -> Option<&str> {
    let rest = trimmed.trim_start_matches('#');
    let level = trimmed.len() - rest.len();
    if level == 0 || level > 6 {
        return None;
    }
    if !rest.is_empty() && !rest.starts_with(' ') {
        // `#hashtag` is body text, not a heading.
        return None;
    }
    let title = rest.trim();
    // A closing sequence of hashes is decoration when a space precedes it.
    let without_closing = title.trim_end_matches('#');
    if without_closing.len() < title.len() && without_closing.ends_with(' ') {
        return Some(without_closing.trim_end());
    }
    Some(title)
}
