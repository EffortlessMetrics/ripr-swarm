//! Before/after validation against one captured inline test-module region.

use std::ops::Range;

use super::{
    InlineTestRegionAuthority, InlineTestRegionRejectReason, InlineTestRegionVerdict, digest_bytes,
    observe,
};

/// Validate that `after` is a pure insertion of test-role items into the
/// exact region named by `authority`, relative to `before`: every byte of the
/// before body survives in order, and the inserted spans hold only new test
/// functions (with optional helper `fn` and `use` companions), comments, and
/// whitespace.
pub(crate) fn validate_inline_test_region_edit(
    before: &str,
    authority: &InlineTestRegionAuthority,
    after: &str,
) -> InlineTestRegionVerdict {
    if digest_bytes(before.as_bytes()) != authority.source_digest {
        return InlineTestRegionVerdict::rejected(
            InlineTestRegionRejectReason::StaleSourceDigest,
            None,
        );
    }
    let body = authority.body_range.clone();
    if body.end > before.len() || body.start > body.end {
        return InlineTestRegionVerdict::rejected(
            InlineTestRegionRejectReason::StaleModuleAnchor,
            Some(body),
        );
    }
    if before.get(body.end..body.end.saturating_add(1)) != Some("}") {
        return InlineTestRegionVerdict::rejected(
            InlineTestRegionRejectReason::StaleModuleAnchor,
            Some(body),
        );
    }

    let Some(module_name) = authority
        .portable
        .module_path
        .rsplit("::")
        .next()
        .filter(|name| !name.is_empty())
    else {
        return InlineTestRegionVerdict::rejected(
            InlineTestRegionRejectReason::MissingRegion,
            None,
        );
    };
    let after_region = match observe::observe_inline_test_region(after, module_name) {
        Ok(region) => region,
        Err(error) => {
            let range = first_changed_range(before, after);
            let reason = error
                .reason()
                .unwrap_or(InlineTestRegionRejectReason::Unparseable);
            return InlineTestRegionVerdict::rejected(reason, range);
        }
    };
    if after_region.module_path != authority.portable.module_path {
        return InlineTestRegionVerdict::rejected(InlineTestRegionRejectReason::WrongModule, None);
    }

    let Some(before_prefix) = before.get(..body.start) else {
        return InlineTestRegionVerdict::rejected(
            InlineTestRegionRejectReason::StaleModuleAnchor,
            Some(body),
        );
    };
    let after_prefix = after.get(..after_region.body_range.start).unwrap_or("");
    if after_prefix != before_prefix {
        let range = first_changed_range(before, after);
        let reason = match range.as_ref() {
            Some(changed) if changed.start < authority.header_range.start => {
                InlineTestRegionRejectReason::ProductionEdit
            }
            _ => InlineTestRegionRejectReason::ModuleDeclarationChanged,
        };
        return InlineTestRegionVerdict::rejected(reason, range);
    }
    let before_suffix = before.get(body.end..).unwrap_or("");
    let after_suffix = after.get(after_region.body_range.end..).unwrap_or("");
    if after_suffix != before_suffix {
        let range = first_changed_range(before, after);
        return InlineTestRegionVerdict::rejected(
            production_or_declaration_reason(before, after, &body),
            range,
        );
    }
    if after_region.cfg_basis_digest != authority.portable.cfg_basis_digest {
        return InlineTestRegionVerdict::rejected(
            InlineTestRegionRejectReason::CfgBasisChanged,
            None,
        );
    }
    if after_region.header_anchor_digest != authority.portable.header_anchor_digest {
        return InlineTestRegionVerdict::rejected(
            InlineTestRegionRejectReason::StaleModuleAnchor,
            None,
        );
    }
    let before_body = match observe::named_module_body(before, &authority.portable.module_path) {
        Ok(body) => body,
        Err(error) => {
            return InlineTestRegionVerdict::rejected(
                error
                    .reason()
                    .unwrap_or(InlineTestRegionRejectReason::MissingRegion),
                None,
            );
        }
    };
    let after_body = match observe::named_module_body(after, &authority.portable.module_path) {
        Ok(body) => body,
        Err(error) => {
            return InlineTestRegionVerdict::rejected(
                error
                    .reason()
                    .unwrap_or(InlineTestRegionRejectReason::Unparseable),
                None,
            );
        }
    };

    match classify_body_delta(&before_body, &after_body) {
        ItemDelta::Admitted => InlineTestRegionVerdict::admitted(),
        ItemDelta::NotARepair => {
            InlineTestRegionVerdict::not_a_repair(InlineTestRegionRejectReason::NonTestSubject)
        }
        ItemDelta::Rejected(reason) => {
            InlineTestRegionVerdict::rejected(reason, first_changed_range(before, after))
        }
    }
}

enum ItemDelta {
    Admitted,
    NotARepair,
    Rejected(InlineTestRegionRejectReason),
}

/// Align the after body with the before body. Every before element must
/// survive in order, byte for byte: whitespace may only grow (its bytes stay
/// a subsequence), and comments and items must reappear unchanged. Anything
/// else in the after body is an insertion, and only whitespace, comments,
/// `fn` items, and `use` items may be inserted. At least one inserted `fn`
/// must carry a recognised test attribute; helpers and `use` companions are
/// admitted only beside one.
fn classify_body_delta(
    before: &[observe::BodyElement],
    after: &[observe::BodyElement],
) -> ItemDelta {
    let mut cursor = BeforeCursor::new(before);
    let mut trivia_lost = false;
    let mut inserted_refusal = None;
    let mut added_test = false;
    for element in after {
        if element.kind == observe::BodyElementKind::Whitespace {
            cursor.consume_whitespace(&element.text);
            continue;
        }
        if cursor.pending_whitespace() && cursor.peek_after_whitespace() == Some(element) {
            // The before whitespace ahead of this element did not survive.
            trivia_lost = true;
            cursor.skip_whitespace();
        }
        if !cursor.pending_whitespace() && cursor.peek() == Some(element) {
            cursor.advance();
            continue;
        }
        let refusal = match element.kind {
            observe::BodyElementKind::Item(observe::ObservedItemKind::TestFn) => {
                added_test = true;
                None
            }
            observe::BodyElementKind::Item(
                observe::ObservedItemKind::Fn | observe::ObservedItemKind::Use,
            )
            | observe::BodyElementKind::Comment => None,
            observe::BodyElementKind::Item(observe::ObservedItemKind::NestedModule) => {
                Some(InlineTestRegionRejectReason::UnsupportedModuleKind)
            }
            observe::BodyElementKind::Item(observe::ObservedItemKind::Other) => {
                Some(InlineTestRegionRejectReason::NonTestSubject)
            }
            observe::BodyElementKind::Other | observe::BodyElementKind::Whitespace => {
                Some(InlineTestRegionRejectReason::NotPureInsertion)
            }
        };
        if inserted_refusal.is_none() {
            inserted_refusal = refusal;
        }
    }
    if cursor.pending_whitespace() {
        trivia_lost = true;
        cursor.skip_whitespace();
    }
    if let Some(reason) = inserted_refusal {
        return ItemDelta::Rejected(reason);
    }
    // The first before element that never reappeared names the refusal: an
    // existing item is rewritten authority; a comment or other trivia is
    // existing body text that was not preserved.
    match cursor
        .remaining()
        .iter()
        .find(|element| element.kind != observe::BodyElementKind::Whitespace)
        .map(|element| element.kind)
    {
        Some(observe::BodyElementKind::Item(_)) => {
            return ItemDelta::Rejected(InlineTestRegionRejectReason::ExistingAuthorityRewritten);
        }
        Some(_) => return ItemDelta::Rejected(InlineTestRegionRejectReason::NotPureInsertion),
        None if !cursor.remaining().is_empty() => trivia_lost = true,
        None => {}
    }
    if trivia_lost {
        return ItemDelta::Rejected(InlineTestRegionRejectReason::NotPureInsertion);
    }
    if added_test {
        ItemDelta::Admitted
    } else {
        ItemDelta::NotARepair
    }
}

/// Position in the before body: the next unmatched element, and for a
/// whitespace element the bytes not yet found in the after body.
struct BeforeCursor<'a> {
    elements: &'a [observe::BodyElement],
    next: usize,
    whitespace_left: &'a str,
}

impl<'a> BeforeCursor<'a> {
    fn new(elements: &'a [observe::BodyElement]) -> Self {
        let mut cursor = Self {
            elements,
            next: 0,
            whitespace_left: "",
        };
        cursor.load();
        cursor
    }

    fn load(&mut self) {
        self.whitespace_left = match self.elements.get(self.next) {
            Some(element) if element.kind == observe::BodyElementKind::Whitespace => {
                element.text.as_str()
            }
            _ => "",
        };
    }

    fn advance(&mut self) {
        self.next += 1;
        self.load();
    }

    fn pending_whitespace(&self) -> bool {
        !self.whitespace_left.is_empty()
    }

    fn skip_whitespace(&mut self) {
        if self.pending_whitespace() {
            self.advance();
        }
    }

    fn peek(&self) -> Option<&'a observe::BodyElement> {
        self.elements.get(self.next)
    }

    fn peek_after_whitespace(&self) -> Option<&'a observe::BodyElement> {
        self.elements.get(self.next + 1)
    }

    /// Greedily find the remaining before whitespace bytes, in order, inside
    /// one after whitespace token; unmatched after bytes are inserted.
    fn consume_whitespace(&mut self, after: &str) {
        if !self.pending_whitespace() {
            return;
        }
        let current: &'a str = self.whitespace_left;
        let mut left = current.char_indices().peekable();
        for character in after.chars() {
            if left
                .peek()
                .is_some_and(|(_, expected)| *expected == character)
            {
                left.next();
            }
        }
        match left.peek() {
            Some((offset, _)) => {
                self.whitespace_left = current.get(*offset..).unwrap_or("");
            }
            None => self.advance(),
        }
    }

    fn remaining(&self) -> &'a [observe::BodyElement] {
        self.elements.get(self.next..).unwrap_or(&[])
    }
}

fn production_or_declaration_reason(
    before: &str,
    after: &str,
    body: &Range<usize>,
) -> InlineTestRegionRejectReason {
    let Some(range) = first_changed_range(before, after) else {
        return InlineTestRegionRejectReason::NotPureInsertion;
    };
    if range.start < body.start {
        return InlineTestRegionRejectReason::ProductionEdit;
    }
    if range.start >= body.end.saturating_add(1) {
        return InlineTestRegionRejectReason::ProductionEdit;
    }
    InlineTestRegionRejectReason::ModuleDeclarationChanged
}

fn first_changed_range(before: &str, after: &str) -> Option<Range<usize>> {
    let prefix = common_prefix_len(before.as_bytes(), after.as_bytes());
    if prefix == before.len() && prefix == after.len() {
        return None;
    }
    let before_tail = before.as_bytes().get(prefix..).unwrap_or(&[]);
    let after_tail = after.as_bytes().get(prefix..).unwrap_or(&[]);
    let suffix = common_suffix_len(before_tail, after_tail);
    Some(prefix..after.len().saturating_sub(suffix).max(prefix + 1))
}

fn common_prefix_len(left: &[u8], right: &[u8]) -> usize {
    left.iter()
        .zip(right)
        .take_while(|(left, right)| left == right)
        .count()
}

fn common_suffix_len(left: &[u8], right: &[u8]) -> usize {
    left.iter()
        .rev()
        .zip(right.iter().rev())
        .take_while(|(left, right)| left == right)
        .count()
        .min(left.len())
        .min(right.len())
}
