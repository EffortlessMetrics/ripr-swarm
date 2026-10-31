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
/// `<=`, `>=`, `<=>`, `lt`, `gt`, `le`, `ge`, `cmp`), normalized. It errs
/// toward finding a comparison (a `<` inside a literal counts): a false
/// positive only withholds `exposed`. A literal next to an arithmetic or
/// concatenation operator (`$n + 1 > 10`, `$x > 10 + $y`) is only part of an
/// operand and is dropped, so a literal elsewhere in the expression never
/// credits a test.
fn ordering_thresholds(text: &str) -> Option<Vec<String>> {
    let chars: Vec<char> = text.chars().collect();
    let mut found = false;
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
        found = true;
        comparisons += 1;
        thresholds.extend(operand_before(&chars, start));
        thresholds.extend(operand_after(&chars, end));
        i = end;
    }
    // Which comparison the change sits on is not known from this text, so a
    // second ordering comparison (`$a > 5 && $b < 100`) would let an input
    // that only hits the other one credit the change: no thresholds.
    if comparisons > 1 {
        thresholds.clear();
    }
    found.then_some(thresholds)
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
    let negative = start > 0
        && chars[start - 1] == '-'
        && (start == 1 || matches!(chars[start - 2], ' ' | '\t' | '(' | ','));
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
    if chars.get(trail).is_some_and(|c| is_arithmetic(*c))
        || (chars.get(trail) == Some(&'x') && !chars.get(trail + 1).is_some_and(|c| is_word(*c)))
    {
        return None;
    }
    numeric_literal(&token, negative)
}

/// An operator that makes a neighboring literal only part of an operand.
fn is_arithmetic(c: char) -> bool {
    matches!(c, '+' | '-' | '*' | '/' | '%' | '.')
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
    let chars = mask_strings_and_comments(expression);
    let needle: Vec<char> = name.chars().collect();
    let mut literals = Vec::new();
    let mut i = 0;
    while i + needle.len() <= chars.len() {
        let before_ok =
            i == 0 || !(is_word(chars[i - 1]) || matches!(chars[i - 1], '$' | '@' | '%'));
        if before_ok
            && chars[i..i + needle.len()] == needle[..]
            && qualifier_matches(&chars, i, package)
        {
            let mut open = i + needle.len();
            while open < chars.len() && chars[open].is_whitespace() {
                open += 1;
            }
            if chars.get(open) == Some(&'(') {
                literals.extend(top_level_literal_args(&chars, open + 1));
            }
            i += needle.len();
        } else {
            i += 1;
        }
    }
    literals
}

/// The expression's characters with quoted spans and `#` comments blanked, so
/// a call spelled inside a string (`'tier_label(1000) is gold'`) or a comment
/// is never read as a call. Quote delimiters stay (argument splitting still
/// sees one quoted argument); their contents become `x`. Comments become
/// spaces. `q`, `qq` and `qw` spans are masked to their closing delimiter. An
/// unterminated span masks to the end: uncertainty only withholds `exposed`.
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
}
