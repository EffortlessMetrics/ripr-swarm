from pathlib import Path

path = Path("crates/ripr/src/analysis/classify/reveal.rs")
text = path.read_text(encoding="utf-8")


def replace_once(old: str, new: str, label: str) -> None:
    global text
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label} anchor count: {count}")
    text = text.replace(old, new, 1)


replace_once(
    "    match_arm_literals: &'a [String],\n"
    "    error_construction_variant: Option<&'a str>,\n",
    "    match_arm_literals: &'a [String],\n"
    "    /// True when the parser-owned arm pattern carries a guard. This slice\n"
    "    /// fails closed until the observed input can be shown to satisfy it.\n"
    "    match_arm_guarded: bool,\n"
    "    error_construction_variant: Option<&'a str>,\n",
    "RevealMatchContext guard field",
)

guard_helper = r'''
/// Whether the parser-owned match-arm pattern includes a guard before `=>`.
///
/// The pattern text is already parser-produced. Comments and string contents
/// are masked before checking the `if` keyword, so `"if" =>` and comments do
/// not create a guard. This slice deliberately fails closed for guarded arms:
/// matching the pattern literal alone does not establish guard satisfaction.
fn match_arm_pattern_has_guard(expression: &str) -> bool {
    let Some((pattern, _)) = expression.split_once("=>") else {
        return false;
    };
    let masked = crate::analysis::extract::mask_comments_and_strings(pattern);
    contains_as_whole_word(&masked, "if")
}

'''
marker = "/// Byte ranges (quotes included) of the string-literal spans in `text`,\n"
if text.count(marker) != 1:
    raise SystemExit(f"guard helper insertion marker count: {text.count(marker)}")
text = text.replace(marker, guard_helper + marker, 1)

marker = "    // For probes whose changed expression constructs an exact error variant\n"
guard_compute = (
    "    // A literal/variant match does not establish that an arm guard evaluated\n"
    "    // true. Preserve the arm as an explicit unverified observation until a\n"
    "    // producer-owned guard witness exists.\n"
    "    let match_arm_guarded = matches!(probe.family, ProbeFamily::MatchArm)\n"
    "        && match_arm_pattern_has_guard(analysis_expression);\n"
)
if text.count(marker) != 1:
    raise SystemExit(f"guard compute marker count: {text.count(marker)}")
text = text.replace(marker, guard_compute + marker, 1)

replace_once(
    "        match_arm_literals: &match_arm_literals,\n"
    "        error_construction_variant: error_construction_variant.as_deref(),\n",
    "        match_arm_literals: &match_arm_literals,\n"
    "        match_arm_guarded,\n"
    "        error_construction_variant: error_construction_variant.as_deref(),\n",
    "production context guard",
)

replace_once(
    "        match_arm_literals,\n"
    "        error_construction_variant,\n",
    "        match_arm_literals,\n"
    "        match_arm_guarded,\n"
    "        error_construction_variant,\n",
    "context destructure guard",
)

owner_doc_start = text.index("/// String literals supplied as inputs to the changed owner:")
owner_end = text.index("\n/// Returns `(matched, has_token_match)`.", owner_doc_start)
owner_block = r'''/// Direct string-literal inputs supplied by the observed owner call.
///
/// Match-arm confirmation is intentionally narrower than arbitrary literal
/// occurrence. Only the two compared operands of `assert_eq!` / `assert_ne!`
/// are observed. Diagnostic arguments are ignored. Within a compared operand,
/// the complete expression must be a syntactically bare owner call with one
/// direct string-literal argument. Qualified paths, methods, wrappers,
/// conditionals, blocks, variables, and transformed/nested inputs fail closed.
fn split_top_level_arguments(text: &str) -> Option<Vec<&str>> {
    let mut arguments = Vec::new();
    let mut start = 0usize;
    let mut stack = Vec::new();
    let mut in_string = false;
    let mut in_char = false;
    let mut escaped = false;

    for (index, ch) in text.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if in_char {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '\'' {
                in_char = false;
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '\'' => in_char = true,
            '(' | '[' | '{' => stack.push(ch),
            ')' => {
                if stack.pop() != Some('(') {
                    return None;
                }
            }
            ']' => {
                if stack.pop() != Some('[') {
                    return None;
                }
            }
            '}' => {
                if stack.pop() != Some('{') {
                    return None;
                }
            }
            ',' if stack.is_empty() => {
                arguments.push(text[start..index].trim());
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }

    if in_string || in_char || !stack.is_empty() {
        return None;
    }
    arguments.push(text[start..].trim());
    Some(arguments)
}

fn matching_parenthesis(text: &str, opening: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut in_char = false;
    let mut escaped = false;

    for (relative, ch) in text[opening..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if in_char {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '\'' {
                in_char = false;
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '\'' => in_char = true,
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(opening + relative);
                }
            }
            _ => {}
        }
    }
    None
}

fn assertion_comparison_operands(text: &str) -> Option<[&str; 2]> {
    let spans = string_span_ranges(text);
    for macro_name in ["assert_eq!", "assert_ne!"] {
        let mut search_from = 0usize;
        while let Some(relative) = text[search_from..].find(macro_name) {
            let start = search_from + relative;
            if spans
                .iter()
                .any(|(span_start, span_end)| start >= *span_start && start < *span_end)
            {
                search_from = start + macro_name.len();
                continue;
            }

            let mut opening = start + macro_name.len();
            while text[opening..]
                .chars()
                .next()
                .is_some_and(char::is_whitespace)
            {
                opening += text[opening..].chars().next()?.len_utf8();
            }
            if text[opening..].chars().next()? != '(' {
                search_from = start + macro_name.len();
                continue;
            }
            let closing = matching_parenthesis(text, opening)?;
            let arguments = split_top_level_arguments(&text[opening + 1..closing])?;
            if arguments.len() < 2 {
                return None;
            }
            return Some([arguments[0], arguments[1]]);
        }
    }
    None
}

fn direct_owner_string_input(expression: &str, owner: &str) -> Option<String> {
    let expression = expression.trim();
    let rest = expression.strip_prefix(owner)?;
    if rest
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }

    let rest = rest.trim_start();
    if !rest.starts_with('(') {
        return None;
    }
    let opening = expression.len() - rest.len();
    let closing = matching_parenthesis(expression, opening)?;
    if !expression[closing + 1..].trim().is_empty() {
        return None;
    }

    let arguments = split_top_level_arguments(&expression[opening + 1..closing])?;
    if arguments.len() != 1 {
        return None;
    }
    let argument = arguments[0].trim();
    let spans = string_span_ranges(argument);
    if spans.len() != 1 || spans[0] != (0, argument.len()) {
        return None;
    }
    let mut literals = rust_string_literals(argument);
    (literals.len() == 1).then(|| literals.remove(0))
}

fn owner_call_literals(text: &str, owner: &str) -> Vec<String> {
    let Some(operands) = assertion_comparison_operands(text) else {
        return Vec::new();
    };
    let mut literals = operands
        .into_iter()
        .filter_map(|operand| direct_owner_string_input(operand, owner))
        .collect::<Vec<_>>();
    literals.sort();
    literals.dedup();
    literals
}
'''
text = text[:owner_doc_start] + owner_block + text[owner_end:]

old = r'''    let has_token_match = if matches!(family, ProbeFamily::MatchArm) {
        match_arm_variants
            .iter()
            .any(|v| contains_as_whole_word(&assertion.text, v))
            || !match_arm_literals.is_empty()
                && owner_callee.is_some_and(|owner| {
                    owner_call_literals(&assertion.text, owner)
                        .iter()
                        .any(|literal| match_arm_literals.contains(literal))
                })
    } else if wrapper_seam {
'''
new = r'''    let has_token_match = if matches!(family, ProbeFamily::MatchArm) {
        !match_arm_guarded
            && (match_arm_variants
                .iter()
                .any(|v| contains_as_whole_word(&assertion.text, v))
                || !match_arm_literals.is_empty()
                    && !import_defeats_owner
                    && !cross_package_defeats_owner
                    && owner_callee.is_some_and(|owner| {
                        owner_call_literals(&assertion.text, owner)
                            .iter()
                            .any(|literal| match_arm_literals.contains(literal))
                    }))
    } else if wrapper_seam {
'''
replace_once(old, new, "match-arm confirmation")

replace_once(
    "            match_arm_literals,\n"
    "            error_construction_variant,\n",
    "            match_arm_literals,\n"
    "            match_arm_guarded: false,\n"
    "            error_construction_variant,\n",
    "test wrapper guard",
)

test_start = text.index("    // Owner-call scoping: longer callee names do not bind, nested calls do.")
test_end = text.index(
    "    // A diagnostic message that spells the owner call binds no input:",
    test_start,
)
owner_tests = r'''    // Only a direct bare owner call in one of the two compared operands
    // supplies an observable literal input. Every other shape fails closed.
    #[test]
    fn owner_call_literal_scope_rejects_longer_callee_names() {
        for assertion in [
            "assert_eq!(reroute(\"sensor\"), 1);",
            "assert_eq!(other::route(\"sensor\"), 1);",
            "assert_eq!(other :: route(\"sensor\"), 1);",
            "assert_eq!(router.route(\"sensor\"), 1);",
            "assert_eq!(router . route(\"sensor\"), 1);",
            "assert_eq!(route(wrap(\"sensor\")), 1);",
            "assert_eq!(route(if false { \"sensor\" } else { \"focused-test\" }), 1);",
            "assert_eq!(route(\"focused-test\"), \"proof\", \"{}\", route(\"sensor\"));",
        ] {
            assert!(
                owner_call_literals(assertion, "route").is_empty(),
                "unsupported owner-call shape unexpectedly supplied an input: {assertion}"
            );
        }
        assert_eq!(
            owner_call_literals("assert_eq!(route(\"sensor\"), \"v\");", "route"),
            vec!["sensor".to_string()]
        );
        assert_eq!(
            owner_call_literals("assert_eq!(\"v\", route(\"sensor\"));", "route"),
            vec!["sensor".to_string()]
        );
    }

    #[test]
    fn match_arm_guard_detection_masks_literals_and_comments() {
        assert!(match_arm_pattern_has_guard(
            "\"sensor\" if kind.len() > 10 => \"sensor-v2\""
        ));
        assert!(!match_arm_pattern_has_guard("\"if\" => \"literal\""));
        assert!(!match_arm_pattern_has_guard(
            "/* if */ \"sensor\" => \"sensor-v2\""
        ));
        assert!(!match_arm_pattern_has_guard("Mode::If => 1"));
    }

    #[test]
    fn match_arm_literal_confirmation_respects_owner_ambiguity_and_guards() {
        let aligned = oracle(
            "assert_eq!(route(\"sensor\"), \"sensor-v2\");",
            OracleKind::ExactValue,
            OracleStrength::Strong,
        );
        let empty = Vec::<String>::new();
        let pattern_literals = vec!["sensor".to_string()];
        let family = ProbeFamily::MatchArm;

        for (guarded, import_defeats_owner, cross_package_defeats_owner, expected) in [
            (false, false, false, true),
            (true, false, false, false),
            (false, true, false, false),
            (false, false, true, false),
            (false, true, true, false),
        ] {
            let context = RevealMatchContext {
                probe_tokens: &empty,
                effect_literals: &empty,
                match_arm_variants: &empty,
                match_arm_literals: &pattern_literals,
                match_arm_guarded: guarded,
                error_construction_variant: None,
                family: &family,
                wrapper_seam: false,
                owner_callee: Some("route"),
            };
            let (_, has_token) = assertion_matches_probe_detail_with_literals(
                &context,
                &aligned,
                2,
                import_defeats_owner,
                cross_package_defeats_owner,
            );
            assert_eq!(
                has_token, expected,
                "wrong confirmation: guarded={guarded} import={import_defeats_owner} cross_package={cross_package_defeats_owner}"
            );
        }
    }

'''
text = text[:test_start] + owner_tests + text[test_end:]

path.write_text(text, encoding="utf-8")
