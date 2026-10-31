//! RIPR-SPEC-0235 rule 22 (#8979): does an exact oracle's own call put a
//! literal input on the threshold of a changed ordering comparison?
//!
//! The packet has no row-owned predicate-input fact (the producer fact tracked
//! by perl-lsp-swarm#17237 and #5562), and ripr does not parse Perl. This is a
//! bounded lexical read of two strings the packet already carries: the change's
//! `changed_observable` and the aligned oracle's `expression`. It only ever
//! withholds `exposed`: a miss reads `static_unknown`, never a credit.

/// What the lexical read established for one aligned oracle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BoundaryInput {
    /// The change is not an ordering comparison; rule 9 alone decides.
    NotAComparison,
    /// The oracle's owner call passes a literal equal to a literal operand of
    /// the changed comparison.
    OnThreshold,
    /// An ordering comparison with no such literal input: unknown.
    Unestablished,
}

pub(super) fn boundary_input_for<'a>(
    changed_observable: &str,
    owner_id: &str,
    oracle_expressions: impl IntoIterator<Item = &'a str>,
) -> BoundaryInput {
    let Some(thresholds) = ordering_thresholds(changed_observable) else {
        return BoundaryInput::NotAComparison;
    };
    let inputs: Vec<String> = oracle_expressions
        .into_iter()
        .flat_map(|expression| owner_call_literal_args(expression, owner_id))
        .collect();
    if thresholds
        .iter()
        .any(|threshold| inputs.contains(threshold))
    {
        BoundaryInput::OnThreshold
    } else {
        BoundaryInput::Unestablished
    }
}

fn is_word(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// `None` when the text has no ordering comparison. Otherwise the numeric
/// literal operands that are a whole side of an ordering operator (`<`, `>`,
/// `<=`, `>=`, `<=>`, `lt`, `gt`, `le`, `ge`, `cmp`), normalized. A `<` or `>`
/// only inside a quoted span is not a comparison: the read withholds rather
/// than treating the change as comparison-free. A false positive only
/// withholds `exposed`. A literal next to an arithmetic or concatenation
/// operator (`$n + 1 > 10`, `$x > 10 + $y`) is only part of an operand and is
/// dropped, so a literal elsewhere in the expression never credits a test.
fn ordering_thresholds(text: &str) -> Option<Vec<String>> {
    let (raw_count, thresholds) = scan_ordering(text);
    // A `>` or `<` only inside a quoted span is not the changed comparison.
    // Masking it away must not become `NotAComparison` (that would let sink
    // alignment credit the row). Differing counts withhold instead.
    let masked: String = mask_strings_and_comments(text).into_iter().collect();
    let (masked_count, _) = scan_ordering(&masked);
    if raw_count != masked_count {
        return (raw_count > 0 || masked_count > 0).then_some(Vec::new());
    }
    (raw_count > 0).then_some(thresholds)
}

fn scan_ordering(text: &str) -> (usize, Vec<String>) {
    let chars: Vec<char> = text.chars().collect();
    let mut comparisons = 0usize;
    let mut thresholds = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let mut span = None;
        if matches!(c, '<' | '>') {
            // `->` is an arrow and `=>` a fat comma, not comparisons.
            let arrow = c == '>' && i > 0 && matches!(chars[i - 1], '-' | '=');
            if !arrow {
                let len =
                    if c == '<' && chars.get(i + 1) == Some(&'=') && chars.get(i + 2) == Some(&'>')
                    {
                        3
                    } else if chars.get(i + 1) == Some(&'=') {
                        2
                    } else {
                        1
                    };
                span = Some((i, i + len));
            }
            if span.is_none() {
                i += 1;
                continue;
            }
        } else if is_word(c) {
            let start = i;
            while i < chars.len() && is_word(chars[i]) {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let named = start > 0 && matches!(chars[start - 1], '$' | '@' | '%' | '&' | '>' | '{');
            let fat_comma = chars[i..]
                .iter()
                .collect::<String>()
                .trim_start()
                .starts_with("=>");
            if !named && !fat_comma && matches!(word.as_str(), "lt" | "gt" | "le" | "ge" | "cmp") {
                span = Some((start, i));
            } else {
                continue;
            }
        }
        let Some((start, end)) = span else {
            i += 1;
            continue;
        };
        comparisons += 1;
        // Only the immediate operand is a call (`length($s) < 5`), not a
        // sibling `foo(1) && $x > 5`.
        if !right_operand_is_call(&chars, end) {
            thresholds.extend(operand_before(&chars, start));
        }
        if !left_operand_is_call(&chars, start) {
            thresholds.extend(operand_after(&chars, end));
        }
        i = end;
    }
    // Which comparison the change sits on is not known from this text, so a
    // second ordering comparison (`$a > 5 && $b < 100`) would let an input
    // that only hits the other one credit the change: no thresholds.
    if comparisons > 1 {
        thresholds.clear();
    }
    (comparisons, thresholds)
}

/// The operand immediately left of an operator is a bare `name(...)` call,
/// not a grouping `($x)` or a sigil/arrow spelling.
fn left_operand_is_call(chars: &[char], operator_start: usize) -> bool {
    let mut end = operator_start;
    while end > 0 && chars[end - 1].is_whitespace() {
        end -= 1;
    }
    if end == 0 || !matches!(chars[end - 1], ')' | ']' | '}') {
        return false;
    }
    let close = chars[end - 1];
    let open = if close == ')' {
        '('
    } else if close == ']' {
        '['
    } else {
        '{'
    };
    let mut depth = 1usize;
    let mut i = end - 1;
    while i > 0 && depth > 0 {
        i -= 1;
        if chars[i] == close {
            depth += 1;
        } else if chars[i] == open {
            depth -= 1;
        }
    }
    if depth != 0 {
        return false;
    }
    let mut name_end = i;
    while name_end > 0 && chars[name_end - 1].is_whitespace() {
        name_end -= 1;
    }
    if name_end == 0 || !is_word(chars[name_end - 1]) {
        return false;
    }
    let mut name_start = name_end;
    while name_start > 0 && is_word(chars[name_start - 1]) {
        name_start -= 1;
    }
    let named =
        name_start > 0 && matches!(chars[name_start - 1], '$' | '@' | '%' | '&' | '>' | '{');
    !named
}

/// The operand immediately right of an operator opens as a bare `name(`.
fn right_operand_is_call(chars: &[char], operator_end: usize) -> bool {
    let mut i = operator_end;
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    if i >= chars.len() || !is_word(chars[i]) {
        return false;
    }
    let start = i;
    while i < chars.len() && is_word(chars[i]) {
        i += 1;
    }
    let named = start > 0 && matches!(chars[start - 1], '$' | '@' | '%' | '&' | '>' | '{');
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    !named && chars.get(i) == Some(&'(')
}

fn operand_before(chars: &[char], operator_start: usize) -> Option<String> {
    let mut end = operator_start;
    while end > 0 && chars[end - 1].is_whitespace() {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && (is_word(chars[start - 1]) || chars[start - 1] == '.') {
        start -= 1;
    }
    // A sigil or subscript makes it a variable or element, not a literal.
    if start > 0 && matches!(chars[start - 1], '$' | '@' | '%' | '&') {
        return None;
    }
    // A `-` is a sign only when no operand precedes it: `$a -5 < $b` and
    // `$n - 5 < $b` are subtraction.
    let negative = start > 0 && chars[start - 1] == '-' && {
        let mut before = start - 1;
        while before > 0 && chars[before - 1].is_whitespace() {
            before -= 1;
        }
        before == 0 || !(is_word(chars[before - 1]) || matches!(chars[before - 1], ')' | ']' | '}'))
    };
    let token: String = chars[start..end].iter().collect();
    let literal_start = if negative { start - 1 } else { start };
    let mut lead = literal_start;
    while lead > 0 && chars[lead - 1].is_whitespace() {
        lead -= 1;
    }
    // `$count + 1 > 10`: the `1` is part of a larger operand, not a side.
    if lead > 0 && is_arithmetic(chars[lead - 1]) {
        return None;
    }
    // `$s x 3 > 100`: a repetition operator before the literal.
    if lead > 1
        && chars[lead - 1] == 'x'
        && !(is_word(chars[lead - 2]) || matches!(chars[lead - 2], '$' | '@' | '%' | '&'))
    {
        return None;
    }
    // `$x | 3 > 5`: a bitwise operator, but not the `||` or `&&` of a condition.
    if lead > 0
        && matches!(chars[lead - 1], '|' | '&')
        && !(lead > 1 && chars[lead - 2] == chars[lead - 1])
    {
        return None;
    }
    numeric_literal(&token, negative)
}

fn operand_after(chars: &[char], operator_end: usize) -> Option<String> {
    let mut start = operator_end;
    while start < chars.len() && chars[start].is_whitespace() {
        start += 1;
    }
    let negative = chars.get(start) == Some(&'-');
    if negative {
        start += 1;
    }
    let mut end = start;
    while end < chars.len() && (is_word(chars[end]) || chars[end] == '.') {
        end += 1;
    }
    let token: String = chars[start..end].iter().collect();
    let mut trail = end;
    while trail < chars.len() && chars[trail].is_whitespace() {
        trail += 1;
    }
    // `$x > 10 + $y`: the `10` is part of a larger operand, not a side.
    let bitwise = chars
        .get(trail)
        .is_some_and(|c| matches!(c, '|' | '&') && chars.get(trail + 1) != Some(c));
    // `$x > 5 x 3` and `$x > 5 x3`: `x` is repetition (`x3` lexes as `x 3`).
    // A following word that is not a digit (`xor`) is a different token.
    let x_repeat = chars.get(trail) == Some(&'x')
        && chars
            .get(trail + 1)
            .is_none_or(|c| !is_word(*c) || c.is_ascii_digit());
    if chars.get(trail).is_some_and(|c| is_arithmetic(*c)) || bitwise || x_repeat {
        return None;
    }
    numeric_literal(&token, negative)
}

/// An operator that makes a neighboring literal only part of an operand.
fn is_arithmetic(c: char) -> bool {
    matches!(c, '+' | '-' | '*' | '/' | '%' | '.' | '^')
}

/// A plain decimal literal (`100`, `1_000`, `0.5`), normalized: no `_`, a
/// leading `-` kept. Anything else (a variable, a hex or exponent form, a
/// constant name) is not a literal this read can compare.
fn numeric_literal(token: &str, negative: bool) -> Option<String> {
    let digits: String = token.chars().filter(|c| *c != '_').collect();
    let mut parts = digits.split('.');
    let whole = parts.next()?;
    let fraction = parts.next();
    let plain = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    if !plain(whole) || parts.next().is_some() || fraction.is_some_and(|f| !plain(f)) {
        return None;
    }
    Some(if negative {
        format!("-{digits}")
    } else {
        digits
    })
}

/// The literal arguments passed directly (at the top level of the argument
/// list) to the changed owner's sub in the oracle expression, normalized. A
/// nested call, a variable or any other argument contributes nothing.
fn owner_call_literal_args(expression: &str, owner_id: &str) -> Vec<String> {
    let Some((package, name)) = owner_sub_name(owner_id) else {
        return Vec::new();
    };
    // Regex literals and heredoc bodies are not masked: a call spelled inside
    // one, or a quote inside a regex, would misread everything after it.
    if has_regex_or_heredoc(expression) {
        return Vec::new();
    }
    let chars = mask_strings_and_comments(expression);
    let needle: Vec<char> = name.chars().collect();
    let mut literals = Vec::new();
    let mut calls = 0usize;
    let mut i = 0;
    while i + needle.len() <= chars.len() {
        let before_ok =
            i == 0 || !(is_word(chars[i - 1]) || matches!(chars[i - 1], '$' | '@' | '%'));
        if before_ok && chars[i..i + needle.len()] == needle[..] {
            let mut open = i + needle.len();
            while open < chars.len() && chars[open].is_whitespace() {
                open += 1;
            }
            if chars.get(open) == Some(&'(') {
                // Count every `name(` outside masked spans, including a method
                // on a variable. Literals still come only from a qualifier that
                // names the owner: counting never grants a call this read
                // cannot pin.
                calls += 1;
                if qualifier_matches(&chars, i, package) {
                    literals.extend(top_level_literal_args(&chars, open + 1));
                }
            }
            i += needle.len();
        } else {
            i += 1;
        }
    }
    // `"@{[ name(2000) ]}"` is masked as a string, so the inner call is
    // invisible above; count it from the original text when it sits in a
    // masked span.
    calls += masked_interpolated_array_calls(expression, &chars, &needle);
    // `is(tier_label(2000), tier_label(5))`: with the sub spelled twice, the
    // read cannot tell the asserted call from the expected-side one.
    if calls > 1 {
        return Vec::new();
    }
    // Argument position is not known: two numeric literals in one call cannot
    // establish which one sits on the threshold.
    if literals.len() > 1 {
        return Vec::new();
    }
    literals
}

/// `name(` calls inside `@{[ ... ]}` interpolations that quoting already
/// masked. Unquoted interpolations are already counted in the masked scan.
fn masked_interpolated_array_calls(original: &str, masked: &[char], needle: &[char]) -> usize {
    let chars: Vec<char> = original.chars().collect();
    if chars.len() != masked.len() {
        return 0;
    }
    let mut calls = 0usize;
    let mut i = 0;
    while i + 2 < chars.len() {
        if chars[i] == '@' && chars[i + 1] == '{' {
            let mut open = i + 2;
            while open < chars.len() && chars[open].is_whitespace() {
                open += 1;
            }
            if chars.get(open) == Some(&'[') && masked.get(i).is_some_and(|c| *c == 'x') {
                let inner_start = open + 1;
                let mut depth = 1usize;
                let mut j = inner_start;
                while j < chars.len() && depth > 0 {
                    if chars[j] == '[' {
                        depth += 1;
                    } else if chars[j] == ']' {
                        depth -= 1;
                    }
                    j += 1;
                }
                if depth == 0 {
                    calls += count_name_paren_calls(&chars[inner_start..j - 1], needle);
                    i = j;
                    continue;
                }
            }
        }
        i += 1;
    }
    calls
}

fn count_name_paren_calls(chars: &[char], needle: &[char]) -> usize {
    let mut calls = 0usize;
    let mut i = 0;
    while i + needle.len() <= chars.len() {
        let before_ok =
            i == 0 || !(is_word(chars[i - 1]) || matches!(chars[i - 1], '$' | '@' | '%'));
        if before_ok && chars[i..i + needle.len()] == needle[..] {
            let mut open = i + needle.len();
            while open < chars.len() && chars[open].is_whitespace() {
                open += 1;
            }
            if chars.get(open) == Some(&'(') {
                calls += 1;
            }
            i += needle.len();
        } else {
            i += 1;
        }
    }
    calls
}

/// Whether the expression holds anything this read does not parse: any `/`
/// (a bare pattern such as `grep /x/, @l`, a `m//`, `s///`, `tr///` or
/// division), a `=~` or `!~` binding, a `<<` heredoc introducer (`<<EOT`,
/// `<< "EOT"`, `<<~EOT`), or a `q`, `qq`, `qw`, `qr`, `m`, `s`, `tr` or `y` word
/// followed by any delimiter. The only exceptions are a word after a sigil or
/// `->` (a variable or method name), a hash key (`{s}`, `s =>`), and a longer
/// name (`tier_label`). It is a denylist-free fail-closed scan, not a Perl
/// parser: it over-withholds (a `/` in a message, a bare `s,`), which only
/// costs a missed credit, never grants `exposed`.
fn has_regex_or_heredoc(text: &str) -> bool {
    if text.contains('/')
        || text.contains("=~")
        || text.contains("!~")
        || text.contains('`')
        || text.contains("$\"")
        || text.contains("$'")
    {
        return true;
    }
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '<' && chars.get(i + 1) == Some(&'<') {
            let mut next = i + 2;
            while next < chars.len() && chars[next].is_whitespace() {
                next += 1;
            }
            if chars.get(next).is_some_and(|n| {
                (is_word(*n) && !n.is_ascii_digit()) || matches!(n, '"' | '\'' | '~')
            }) {
                return true;
            }
            i += 2;
        } else if is_word(c) {
            let start = i;
            while i < chars.len() && is_word(chars[i]) {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let named = start > 0
                && (matches!(chars[start - 1], '$' | '@' | '%' | '&')
                    || (start > 1 && chars[start - 2] == '-' && chars[start - 1] == '>'));
            if !named
                && matches!(
                    word.as_str(),
                    "q" | "qq" | "qw" | "qx" | "qr" | "m" | "s" | "tr" | "y"
                )
            {
                let had_whitespace = i < chars.len() && chars[i].is_whitespace();
                let mut open = i;
                while open < chars.len() && chars[open].is_whitespace() {
                    open += 1;
                }
                let Some(&delimiter) = chars.get(open) else {
                    continue;
                };
                let fat_comma = delimiter == '=' && chars.get(open + 1) == Some(&'>');
                let hash_key = delimiter == '}' && start > 0 && chars[start - 1] == '{';
                // `q x ... x`: a word-character delimiter is not masked, so
                // the expression is unparsed rather than read as a call.
                if had_whitespace
                    && is_word(delimiter)
                    && matches!(word.as_str(), "q" | "qq" | "qw")
                {
                    return true;
                }
                if !is_word(delimiter) && !fat_comma && !hash_key {
                    return true;
                }
            }
        } else {
            i += 1;
        }
    }
    false
}

/// The expression's characters with quoted spans and `#` comments blanked, so
/// a call spelled inside a string (`'tier_label(1000) is gold'`) or a comment
/// is never read as a call. Quote delimiters stay (argument splitting still
/// sees one quoted argument); their contents become `x`. Comments become
/// spaces. `q`, `qq` and `qw` spans are masked to their closing delimiter. An
/// unterminated span masks to the end, which withholds rather than credits.
fn mask_strings_and_comments(text: &str) -> Vec<char> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = chars.clone();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' || c == '"' {
            i = mask_span(&chars, &mut out, i + 1, c, c);
        } else if c == '#' && !(i > 0 && chars[i - 1] == '$') {
            while i < chars.len() && chars[i] != '\n' {
                out[i] = ' ';
                i += 1;
            }
        } else if is_word(c) {
            let start = i;
            while i < chars.len() && is_word(chars[i]) {
                i += 1;
            }
            // A word after a sigil, `->` or `::` is a variable, method or
            // name, not a quote-like operator.
            let named = start > 0 && matches!(chars[start - 1], '$' | '@' | '%' | '&' | '>' | ':');
            let word: String = chars[start..i].iter().collect();
            if !named && matches!(word.as_str(), "q" | "qq" | "qw") {
                let mut open = i;
                while open < chars.len() && chars[open].is_whitespace() {
                    open += 1;
                }
                if let Some(&delimiter) = chars.get(open)
                    && matches!(delimiter, '(' | '[' | '{' | '<' | '/' | '|' | '!')
                {
                    let close = match delimiter {
                        '(' => ')',
                        '[' => ']',
                        '{' => '}',
                        '<' => '>',
                        other => other,
                    };
                    i = mask_span(&chars, &mut out, open + 1, delimiter, close);
                }
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Masks from `from` to the closing delimiter (nesting counted for brackets),
/// returning the index after it. A backslash masks the next character too.
fn mask_span(chars: &[char], out: &mut [char], from: usize, open: char, close: char) -> usize {
    let mut depth = 0usize;
    let mut i = from;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            out[i] = 'x';
            if let Some(next) = out.get_mut(i + 1) {
                *next = 'x';
            }
            i += 2;
            continue;
        }
        if c == close && depth == 0 {
            return i + 1;
        }
        if open != close && c == open {
            depth += 1;
        } else if open != close && c == close {
            depth -= 1;
        }
        out[i] = 'x';
        i += 1;
    }
    chars.len()
}

/// A call spelled `Other::name(` is another package's sub: its qualifier must
/// name the owner's package. A method call (`Pkg->name(`) needs a bareword
/// receiver naming the owner's package; a variable or other receiver has an
/// unknown package and establishes nothing. An unqualified call is accepted.
fn qualifier_matches(chars: &[char], name_start: usize, package: Option<&str>) -> bool {
    if name_start >= 2 && chars[name_start - 2] == '-' && chars[name_start - 1] == '>' {
        let mut end = name_start - 2;
        while end > 0 && chars[end - 1].is_whitespace() {
            end -= 1;
        }
        let mut start = end;
        while start > 0 && (is_word(chars[start - 1]) || chars[start - 1] == ':') {
            start -= 1;
        }
        if start > 0 && matches!(chars[start - 1], '$' | '@' | '%' | '&') {
            return false;
        }
        let receiver: String = chars[start..end].iter().collect();
        return package == Some(receiver.as_str());
    }
    if name_start < 2 || chars[name_start - 1] != ':' || chars[name_start - 2] != ':' {
        return true;
    }
    let mut start = name_start - 2;
    while start > 0 && (is_word(chars[start - 1]) || chars[start - 1] == ':') {
        start -= 1;
    }
    let qualifier: String = chars[start..name_start - 2].iter().collect();
    package == Some(qualifier.as_str())
}

/// The package and sub name from `owner:<file>:sub:<Package::name>:<start>-<end>` or the
/// older `perl:<file>::<Package::name>` spelling. A package or any other owner
/// kind has no sub name.
fn owner_sub_name(owner_id: &str) -> Option<(Option<&str>, &str)> {
    let qualified = if let Some((_, tail)) = owner_id.split_once(":sub:") {
        match tail.rsplit_once(':') {
            Some((qualified, span))
                if span.split_once('-').is_some_and(|(start, end)| {
                    !start.is_empty()
                        && !end.is_empty()
                        && start.chars().chain(end.chars()).all(|c| c.is_ascii_digit())
                }) =>
            {
                qualified
            }
            _ => tail,
        }
    } else {
        owner_id.strip_prefix("perl:")?.split_once("::")?.1
    };
    let (package, name) = match qualified.rsplit_once("::") {
        Some((package, name)) => (Some(package), name),
        None => (None, qualified),
    };
    (!name.is_empty() && name.chars().all(is_word)).then_some((package, name))
}

fn top_level_literal_args(chars: &[char], mut i: usize) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        if let Some(open) = quote {
            current.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == open {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => {
                quote = Some(c);
                current.push(c);
            }
            '(' | '[' | '{' => {
                depth += 1;
                current.push(c);
            }
            ')' | ']' | '}' if depth > 0 => {
                depth -= 1;
                current.push(c);
            }
            ')' => {
                args.push(std::mem::take(&mut current));
                break;
            }
            ',' if depth == 0 => args.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    args.iter()
        .filter_map(|arg| {
            let arg = arg.trim();
            let (negative, rest) = match arg.strip_prefix('-') {
                Some(rest) => (true, rest.trim_start()),
                None => (false, arg),
            };
            numeric_literal(rest, negative)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: &str = "owner:lib/Shop/Pricing.pm:sub:Shop::Pricing::tier_label:473-612";

    fn input(observable: &str, expression: &str) -> BoundaryInput {
        boundary_input_for(observable, OWNER, [expression])
    }

    #[test]
    fn far_inputs_are_not_on_the_threshold() {
        assert_eq!(
            input(
                "'gold' if 1000 <= $points",
                "is(Shop::Pricing::tier_label(2000), 'gold', 'big spenders are gold')"
            ),
            BoundaryInput::Unestablished
        );
        assert_eq!(
            input("$points >= 1000", "is(tier_label(10), 'bronze')"),
            BoundaryInput::Unestablished
        );
    }

    #[test]
    fn a_literal_input_equal_to_the_threshold_is_on_it() {
        for (observable, expression) in [
            ("'gold' if 1000 <= $points", "is(tier_label(1000), 'gold')"),
            (
                "$points >= 1000",
                "is(Shop::Pricing::tier_label(1_000), 'gold')",
            ),
            ("$amount < 25", "is(tier_label(undef, 25), 5, 'mid')"),
            ("$n > -5", "is(Shop::Pricing->tier_label(-5), 1)"),
            ("$a lt 7", "is(tier_label( 7 ), 1)"),
        ] {
            assert_eq!(
                input(observable, expression),
                BoundaryInput::OnThreshold,
                "{observable} / {expression}"
            );
        }
    }

    #[test]
    fn only_a_literal_beside_the_operator_is_a_threshold() {
        // 5 and 10 are branch values, not comparison operands.
        assert_eq!(
            input("$amount >= 100 ? 5 : 10", "is(tier_label(5), 5)"),
            BoundaryInput::Unestablished
        );
        // The expected value, a message and a nested call are not inputs.
        for expression in [
            "is(tier_label(2000), 1000, 'gold')",
            "is(tier_label('1000'), 1)",
            "is(tier_label(helper(1000)), 1)",
            "is(tier_label($x), 1) # 1000",
            "is(other_label(1000), 1)",
            "is(retier_label(1000), 1)",
            "is(Other::Pkg::tier_label(1000), 1)",
            "is($tier_label(1000), 1)",
            "is($obj->tier_label(1000), 1)",
            "is(Other->tier_label(1000), 1)",
        ] {
            assert_eq!(
                input("$points >= 1000", expression),
                BoundaryInput::Unestablished,
                "{expression}"
            );
        }
    }

    #[test]
    fn any_exact_oracle_of_the_test_may_supply_the_threshold_input() {
        assert_eq!(
            boundary_input_for(
                "5  if $amount < 25",
                "owner:lib/App/Discount.pm:sub:App::Discount::discount:51-174",
                [
                    "is(App::Discount::discount(undef, 5), 0, 'low')",
                    "is(App::Discount::discount(undef, 25), 5, 'mid')",
                ]
            ),
            BoundaryInput::OnThreshold
        );
        assert_eq!(
            boundary_input_for(
                "5  if $amount < 25",
                "owner:lib/App/Discount.pm:sub:App::Discount::discount:51-174",
                ["is(Other::Mod::discount(undef, 25), 5, 'mid')"]
            ),
            BoundaryInput::Unestablished
        );
    }

    #[test]
    fn non_comparisons_are_left_to_sink_alignment() {
        for observable in [
            "$amount * 0.9",
            "$obj->total",
            "{ lt => 1 }",
            "$h{gt}",
            "$a eq $b",
            "$a == 100",
        ] {
            assert_eq!(
                input(observable, "is(tier_label(100), 1)"),
                BoundaryInput::NotAComparison,
                "{observable}"
            );
        }
    }

    #[test]
    fn the_older_owner_id_spelling_names_the_sub() {
        assert_eq!(
            boundary_input_for(
                "$n >= 100",
                "perl:lib/My/App.pm::My::App::discount",
                ["is(discount(100), 10)"]
            ),
            BoundaryInput::OnThreshold
        );
    }

    #[test]
    fn an_owner_without_a_recoverable_sub_name_establishes_nothing() {
        for owner in [
            "owner:lib/A.pm:package:A:0-21",
            "owner:lib/A.pm:sub:A::f:x-9",
            "perl:lib/A.pm",
            "",
        ] {
            assert_eq!(
                boundary_input_for("$n >= 1000", owner, ["is(f(1000), 1)"]),
                BoundaryInput::Unestablished,
                "{owner}"
            );
        }
    }

    #[test]
    fn a_call_inside_a_string_or_comment_is_not_an_input() {
        for expression in [
            "is(tier_label(2000), 'gold', 'tier_label(1000) is gold')",
            "is(tier_label(2000), \"gold\", \"tier_label(1000) is gold\")",
            "is(tier_label(2000), 'gold') # tier_label(1000)",
            "is(tier_label(2000), q(tier_label(1000)))",
            "is(tier_label(2000), qq{tier_label(1000)})",
            "is(tier_label(2000), 'it\\'s tier_label(1000)')",
            "is(tier_label(2000), 'unterminated tier_label(1000)",
        ] {
            assert_eq!(
                input("$points >= 1000", expression),
                BoundaryInput::Unestablished,
                "{expression}"
            );
        }
        // Controls: a real call next to a string or comment still counts, and
        // `$#` is not a comment.
        for expression in [
            "is(tier_label(1000), 'gold', 'tier_label(5)') # note",
            "is(tier_label(1000), $#items)",
            "is('x', tier_label(1000))",
        ] {
            assert_eq!(
                input("$points >= 1000", expression),
                BoundaryInput::OnThreshold,
                "{expression}"
            );
        }
    }

    #[test]
    fn a_literal_inside_a_larger_operand_is_not_a_threshold() {
        // The 1, 10 and 100 are parts of arithmetic operands.
        for (observable, argument) in [
            ("$count + 1 > 10", 1),
            ("$x > 10 + $y", 10),
            ("$total / 100 >= 5", 100),
            ("$a - 3 < $b", 3),
            ("$a * 2 lt $b", 2),
            ("$a > 7 . $b", 7),
            ("$a > 4 ** 2", 4),
        ] {
            assert_eq!(
                input(observable, &format!("is(tier_label({argument}), 1)")),
                BoundaryInput::Unestablished,
                "{observable}"
            );
        }
        // A whole side stays a threshold, even beside an arithmetic other side.
        for (observable, argument) in [
            ("$count + 1 > 10", 10),
            ("$total / 100 >= 5", 5),
            ("10 < $x", 10),
            ("$x < -3", -3),
            ("($x) >= 100", 100),
            ("$ok && $x > 100 ? 1 : 2", 100),
        ] {
            assert_eq!(
                input(observable, &format!("is(tier_label({argument}), 1)")),
                BoundaryInput::OnThreshold,
                "{observable}"
            );
        }
    }

    #[test]
    fn a_second_ordering_comparison_withholds_every_threshold() {
        // 5 only hits `$a > 5`; which comparison changed is unknown.
        for observable in ["$a > 5 && $b < 100", "$x lt 3 || $y >= 9"] {
            assert_eq!(
                input(observable, "is(tier_label(5), 1)"),
                BoundaryInput::Unestablished,
                "{observable}"
            );
            assert_eq!(
                input(observable, "is(tier_label(100), 1)"),
                BoundaryInput::Unestablished,
                "{observable}"
            );
        }
    }

    #[test]
    fn each_remaining_leak_withholds_on_its_own() {
        // A: the owner called on the expected side as well.
        assert_eq!(
            input("$x > 5", "is(tier_label(2000), tier_label(5), 'msg')"),
            BoundaryInput::Unestablished
        );
        // C: a bitwise neighbour, on either side.
        for observable in [
            "$x > 5 | 3",
            "$x > 5 ^ 3",
            "$x > 5 & 3",
            "3 | 5 < $x",
            "$m ^ 5 < $x",
        ] {
            assert_eq!(
                input(observable, "is(tier_label(5), 1)"),
                BoundaryInput::Unestablished,
                "{observable}"
            );
        }
        // D: a repetition operator before the literal.
        assert_eq!(
            input("$s x 3 > 100", "is(tier_label(3), 1)"),
            BoundaryInput::Unestablished
        );
        // E: command and quote-like forms that run or build text.
        for expression in [
            "is(qx(tier_label(5)), 1)",
            "is(qx,tier_label(5),, 1)",
            "is(`tier_label(5)`, 1)",
            "is(1, $\" . \"tier_label(5)\")",
            "is(1, $' . 'tier_label(5)')",
        ] {
            assert_eq!(
                input("$x >= 5", expression),
                BoundaryInput::Unestablished,
                "{expression}"
            );
        }
        // Controls: `&&` and `||` conditions and a single owner call keep the read.
        for (observable, expression) in [
            ("$ok && $x > 5 || $y", "is(tier_label(5), 1)"),
            ("$x > 5 && $ok", "is(tier_label(5), 1)"),
            ("$b > -5", "is(tier_label(-5), 1)"),
            ("$x >= 5", "is(tier_label(5), $x, $s x 3)"),
        ] {
            assert_eq!(
                input(observable, expression),
                BoundaryInput::OnThreshold,
                "{observable}"
            );
        }
    }

    #[test]
    fn method_receiver_and_interpolated_duplicate_calls_withhold() {
        // Item 1 (#11394): the duplicate guard counted only qualifier-matching
        // owner calls, so a method on a variable plus a bare call still read
        // OnThreshold, as did a call inside `"@{[ ... ]}"`.
        assert_eq!(
            input("$x > 5", "is($o->tier_label(2000), tier_label(5))"),
            BoundaryInput::Unestablished
        );
        assert_eq!(
            input("$x > 5", r#"is("@{[ tier_label(2000) ]}", tier_label(5))"#),
            BoundaryInput::Unestablished
        );
        // A differently named method is not a second `tier_label(`.
        assert_eq!(
            input("$x > 5", "is($o->other_label(2000), tier_label(5))"),
            BoundaryInput::OnThreshold
        );
    }

    #[test]
    fn repetition_x_glued_to_digits_withholds() {
        // Item 2 (#11394): `5 x3` lexes as `x 3`, but the after-operand scan
        // only rejected `x` when the next character was not a word.
        assert_eq!(
            input("$x > 5 x3", "is(tier_label(5), 1)"),
            BoundaryInput::Unestablished
        );
    }

    #[test]
    fn quote_like_word_character_delimiter_withholds() {
        // Item 3 (#11394): `q`/`qq`/`qw` plus whitespace plus a word-character
        // delimiter is neither masked nor already in the unparsed denylist.
        assert_eq!(
            input("$x > 5", "is(q x tier_label(5) x, 1)"),
            BoundaryInput::Unestablished
        );
        assert_eq!(
            input("$x > 5", "is(qq x tier_label(5) x, 1)"),
            BoundaryInput::Unestablished
        );
        assert_eq!(
            input("$x > 5", "is(qw x tier_label(5) x, 1)"),
            BoundaryInput::Unestablished
        );
    }

    #[test]
    fn extra_literal_argument_or_call_side_withholds() {
        // Item 4 (#11394): argument position is not known, so a second numeric
        // literal in the owner call, or a call on the compared side, cannot
        // establish the threshold.
        assert_eq!(
            input("$p >= 100", "is(tier_label(5, 100), 1)"),
            BoundaryInput::Unestablished
        );
        assert_eq!(
            input("length($s) < 5", "is(tier_label(5), 1)"),
            BoundaryInput::Unestablished
        );
        assert_eq!(
            input("5 < length($s)", "is(tier_label(5), 1)"),
            BoundaryInput::Unestablished
        );
        // A call in a sibling conjunct is not the compared operand.
        assert_eq!(
            input("foo(1) && $x > 5", "is(tier_label(5), 1)"),
            BoundaryInput::OnThreshold
        );
        assert_eq!(
            input("$x > 5 && foo(1)", "is(tier_label(5), 1)"),
            BoundaryInput::OnThreshold
        );
        assert_eq!(
            input("($x) >= 100", "is(tier_label(100), 1)"),
            BoundaryInput::OnThreshold
        );
        assert_eq!(
            input("$amount < 25", "is(tier_label(undef, 25), 5, 'mid')"),
            BoundaryInput::OnThreshold
        );
    }

    #[test]
    fn comparison_inside_observable_string_withholds() {
        // Item 5 (#11394): a `>` inside a quoted observable is not a changed
        // ordering comparison. Clearing it must stay Unestablished (not
        // NotAComparison), so sink alignment cannot take the row.
        assert_eq!(
            input("$x == 3 ? 'a > 5' : 0", "is(tier_label(5), 1)"),
            BoundaryInput::Unestablished
        );
        assert_eq!(
            input("'gold' if 1000 <= $points", "is(tier_label(1000), 'gold')"),
            BoundaryInput::OnThreshold
        );
    }

    #[test]
    fn each_unparsed_construct_withholds_the_owner_call_on_its_own() {
        // A real far call plus 1000 only inside a non-call span; none of
        // these has `=~`, so each branch of the guard stands alone.
        for expression in [
            "is(1, 1); grep /tier_label(1000)/, @x",
            "is(1, 1); split /tier_label(1000)/, $x",
            "is(1, 1); my @l = map { m/tier_label(1000)/ } @x",
            "is(1, 1); $v ? 1 :s/a/tier_label(1000)/",
            "is(1, 1); {m,tier_label(1000),}",
            "is(1, 1); $v ? 1 :s,a,tier_label(1000),",
            "is(1, 1); s=a=tier_label(1000)=",
            "is(1, 1); y;a;tier_label(1000);",
            "is(1, 1); qq^tier_label(1000)^",
            "is(1, 1); q,tier_label(1000),",
            "is(1, <<EOT)\ntier_label(1000)\nEOT",
            "is(1, << \"EOT\")\ntier_label(1000)\nEOT",
            "is(1, <<'EOT')\ntier_label(1000)\nEOT",
            "is(1, <<~EOT)\ntier_label(1000)\nEOT",
            "is(1, 1) =~ tier_label(1000)",
            "is(1, 1) !~ tier_label(1000)",
        ] {
            assert_eq!(
                input("$points >= 1000", expression),
                BoundaryInput::Unestablished,
                "{expression}"
            );
        }
        // Each quote-like word on its own, with a delimiter the quote mask
        // does not know, so only the word list can withhold it.
        for word in ["q", "qq", "qw", "qr", "m", "s", "tr", "y"] {
            let expression = format!("is(1, 1); {word};tier_label(1000);");
            assert_eq!(
                input("$points >= 1000", &expression),
                BoundaryInput::Unestablished,
                "{expression}"
            );
            let spaced = format!("is(1, 1); {word} ,tier_label(1000),");
            assert_eq!(
                input("$points >= 1000", &spaced),
                BoundaryInput::Unestablished,
                "{spaced}"
            );
        }
        // Controls: names that merely look like the words keep the read.
        for expression in [
            "is(tier_label(1000), $s, $y, $m, $q, $tr)",
            "is(tier_label(1000), $h{s}, $obj->y, $obj->s(1), $obj ->m)",
            "is(tier_label(1000), { s => 1, y => 2, m => 3 }, {s})",
            "is(tier_label(1000), tests(1), ms(1), sy(1), $x <=> $y, 1 << 2)",
        ] {
            assert_eq!(
                input("$points >= 1000", expression),
                BoundaryInput::OnThreshold,
                "{expression}"
            );
        }
    }

    #[test]
    fn a_call_inside_a_regex_or_heredoc_is_not_an_input() {
        for expression in [
            "like($s, qr/tier_label(1000)/)",
            "ok($s =~ m{tier_label(1000)})",
            "is(tier_label(2000),1,'x'); $s =~ s/a/tier_label(1000)/",
            "is(tier_label(2000), 1); $s !~ /tier_label(1000)/",
            "like($s, qr/it's/), is(tier_label(1000),'a','x')",
            "like($s, qr/it's/), is(tier_label(2000),'a','tier_label(1000)')",
            "is(tier_label(1000), <<EOT)\nbody\nEOT",
            "is(tier_label(1000), <<\"EOT\")\nbody\nEOT",
            "is(tier_label(1000), <<~EOT)\nbody\nEOT",
            "is(tier_label(2000), 1); ($t = $u) =~ tr/a/b/; tier_label(1000)",
        ] {
            assert_eq!(
                input("$points >= 1000", expression),
                BoundaryInput::Unestablished,
                "{expression}"
            );
        }
        assert_eq!(
            input(
                "$p >= 100",
                "is(tier_label(5), <<EOT)\ntier_label(100)\nEOT"
            ),
            BoundaryInput::Unestablished
        );
        // Controls: `s`, `m`, `y`, `tr` as variables, hash keys, methods or
        // inside longer words are not pattern operators.
        for expression in [
            "is(tier_label(1000), $s, $y, $m)",
            "is(tier_label(1000), { s => 1, y => 2, m => 3 })",
            "is(tier_label(1000), $h{s}, $obj->y, $obj->s(1))",
            "is(tier_label(1000), 'gold', 'tr is a word')",
            "is(tier_label(1000), 1 << 2)",
        ] {
            assert_eq!(
                input("$points >= 1000", expression),
                BoundaryInput::OnThreshold,
                "{expression}"
            );
        }
    }

    #[test]
    fn a_minus_after_an_operand_is_subtraction_not_a_sign() {
        for observable in [
            "$a -5 < $b",
            "$a - 5 < $b",
            "$f(1) -5 < $b",
            "$a[0] -5 < $b",
        ] {
            for argument in ["-5", "5"] {
                assert_eq!(
                    input(observable, &format!("is(tier_label({argument}), 1)")),
                    BoundaryInput::Unestablished,
                    "{observable} / {argument}"
                );
            }
        }
        for observable in ["$b > -5", "($b) > -5", "$b < -5 ? 1 : 0"] {
            assert_eq!(
                input(observable, "is(tier_label(-5), 1)"),
                BoundaryInput::OnThreshold,
                "{observable}"
            );
        }
        assert_eq!(
            input("-5 < $b", "is(tier_label(-5), 1)"),
            BoundaryInput::OnThreshold
        );
    }
}
