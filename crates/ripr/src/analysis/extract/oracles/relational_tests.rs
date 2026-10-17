use super::{
    classify_assertion, extract_assertions, extract_line_scanned_oracles, is_inequality_assertion,
};
use crate::analysis::facts::OracleFact;
use crate::domain::{OracleKind, OracleStrength};

fn require_kind(
    fact: &OracleFact,
    kind: OracleKind,
    strength: OracleStrength,
) -> Result<(), String> {
    if fact.kind != kind || fact.strength != strength {
        return Err(format!(
            "{}: expected {kind:?}/{strength:?}, got {:?}/{:?}",
            fact.text, fact.kind, fact.strength
        ));
    }
    Ok(())
}

#[test]
fn relational_scalar_fields_do_not_inherit_observer_or_mock_names() -> Result<(), String> {
    let fields = [
        "published_payload_bytes",
        "suppressed_payload_bytes",
        "counter",
        "state_count",
        "expect_published_count",
    ];
    for field in fields {
        let text = format!("assert!(plan.{field} > 0);");
        let facts = extract_assertions(&text, 3096);
        if facts.len() != 1 {
            return Err(format!(
                "expected one actual fact for {text}, got {facts:?}"
            ));
        }
        let fact = facts.first().ok_or("missing actual predicate fact")?;
        if fact.line != 3096
            || fact.text != text
            || !fact.observed_tokens.iter().any(|token| token == field)
            || !fact.observed_tokens.iter().any(|token| token == "plan")
        {
            return Err(format!("predicate source identity lost: {fact:?}"));
        }
        require_kind(fact, OracleKind::RelationalCheck, OracleStrength::Weak)?;
    }
    Ok(())
}

#[test]
fn line_scanned_relational_predicate_preserves_recognition_boundary() -> Result<(), String> {
    let body = format!(
        "assert!(unchanged.suppressed_payload_bytes > 0);\n{}assert!(changed_plan.published_payload_bytes > 0);\nmock_service.expect_publish().times(1);",
        "\n".repeat(12)
    );
    let parsed = extract_assertions(&body, 3096);
    if parsed.len() != 3 {
        return Err(format!(
            "parsed route must emit three real facts: {parsed:?}"
        ));
    }
    for (position, line) in [(0, 3096), (1, 3109)] {
        let fact = parsed.get(position).ok_or("missing parsed twin")?;
        if fact.line != line {
            return Err(format!("parsed line changed: {fact:?}"));
        }
        require_kind(fact, OracleKind::RelationalCheck, OracleStrength::Weak)?;
    }
    let scanned = extract_line_scanned_oracles(&body, 3096);
    if scanned.len() != 2 {
        return Err(format!(
            "fallback must retain published/mock only, not general asserts: {scanned:?}"
        ));
    }
    let published = scanned
        .first()
        .ok_or("missing recognized published predicate")?;
    if published.line != 3109
        || !published
            .observed_tokens
            .iter()
            .any(|token| token == "published_payload_bytes")
    {
        return Err(format!("fallback predicate identity lost: {published:?}"));
    }
    require_kind(published, OracleKind::RelationalCheck, OracleStrength::Weak)?;
    let mock = scanned.get(1).ok_or("missing real mock expectation")?;
    if mock.line != 3110 {
        return Err(format!("mock line changed: {mock:?}"));
    }
    require_kind(mock, OracleKind::MockExpectation, OracleStrength::Medium)
}

#[test]
fn relational_messages_and_call_chains_do_not_steal_oracle_kind() -> Result<(), String> {
    for text in [
        "assert!((plan.published_payload_bytes > 0));",
        "assert!(amount > 0, \"published assert!(plan.counter > 0)\");",
    ] {
        let facts = extract_assertions(text, 40);
        if facts.len() != 1 {
            return Err(format!("condition must emit one real fact: {facts:?}"));
        }
        require_kind(
            facts.first().ok_or("missing condition fact")?,
            OracleKind::RelationalCheck,
            OracleStrength::Weak,
        )?;
    }
    for text in [
        "mock_service.expect_publish().times(1);",
        "assert!(mock_service.expect_publish().times(1) > 0);",
        "assert!(event.published);",
    ] {
        let facts = extract_assertions(text, 41);
        if facts.len() != 1 {
            return Err(format!(
                "mock/observer control must emit one real fact: {facts:?}"
            ));
        }
        require_kind(
            facts.first().ok_or("missing retained mock control")?,
            OracleKind::MockExpectation,
            OracleStrength::Medium,
        )?;
    }
    // Quoted/raw assertion spellings and extra tokens are not a complete outer
    // scalar assertion. Retain the pre-existing generic observer classification.
    for text in [
        "\"assert!(plan.published > 0)\"",
        "r#\"assert!(plan.published > 0)\"#",
        "wrapper(assert!(plan.published > 0));",
        "// assert!(plan.published > 0)",
        "assert!(plan.published > 0) trailing",
        "assert!(plan.published > 0",
    ] {
        let classification = classify_assertion(text);
        if classification.kind == OracleKind::RelationalCheck {
            return Err(format!(
                "new scalar matcher must not admit non-outer macro text: {text}"
            ));
        }
    }
    Ok(())
}

#[test]
fn scalar_admission_preserves_diagnostic_projection_and_stronger_oracles() -> Result<(), String> {
    for text in [
        "assert!(plan.published_payload_bytes >= 1);",
        "assert!(plan.published_payload_bytes < 1);",
        "assert!(0 <= plan.published_payload_bytes);",
        "assert!(0 < plan.expect_published_count);",
        "assert!(plan.published_payload_bytes > 0, \"{:?}\", unrelated.is_err());",
        "ensure!(plan.published_payload_bytes > 0, \"{:?}\", unrelated.is_err());",
    ] {
        let classification = classify_assertion(text);
        if classification.kind != OracleKind::RelationalCheck
            || classification.strength != OracleStrength::Weak
        {
            return Err(format!(
                "scalar condition changed: {text}: {classification:?}"
            ));
        }
    }
    for (text, kind, strength) in [
        (
            "assert_eq!(plan.published_payload_bytes, 7, \"{:?}\", unrelated.is_err());",
            OracleKind::ExactValue,
            OracleStrength::Strong,
        ),
        (
            "assert_matches!(result, Err(ParseError::InvalidDigit));",
            OracleKind::ExactErrorVariant,
            OracleStrength::Strong,
        ),
        (
            "assert!(result.is_err());",
            OracleKind::BroadError,
            OracleStrength::Weak,
        ),
        (
            "assert_snapshot!(published);",
            OracleKind::Snapshot,
            OracleStrength::Medium,
        ),
        (
            "assert!(plan.published.is_some());",
            OracleKind::SmokeOnly,
            OracleStrength::Smoke,
        ),
        (
            "mock_service.expect_publish().times(1);",
            OracleKind::MockExpectation,
            OracleStrength::Medium,
        ),
        (
            "assert!(event.published);",
            OracleKind::MockExpectation,
            OracleStrength::Medium,
        ),
    ] {
        let classification = classify_assertion(text);
        if classification.kind != kind || classification.strength != strength {
            return Err(format!(
                "retained oracle changed: {text}: {classification:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn scalar_parentheses_do_not_admit_blocks_or_arrays() -> Result<(), String> {
    for text in [
        "assert!({ plan.published_payload_bytes > 0 });",
        "assert!([plan.published_payload_bytes > 0]);",
        "assert!(({ plan.published_payload_bytes > 0 }));",
        "assert!(([plan.published_payload_bytes > 0]));",
    ] {
        let classification = classify_assertion(text);
        if classification.kind != OracleKind::MockExpectation
            || classification.strength != OracleStrength::Medium
        {
            return Err(format!(
                "non-parenthesis wrapper admitted: {text}: {classification:?}"
            ));
        }
    }
    let text = "assert!(((plan.published_payload_bytes > 0)));";
    let classification = classify_assertion(text);
    if classification.kind != OracleKind::RelationalCheck
        || classification.strength != OracleStrength::Weak
    {
        return Err(format!("nested parentheses lost: {classification:?}"));
    }
    Ok(())
}

/// #7558, RIPR-SPEC-0231 rule 1: an `ensure!` condition earns exact credit
/// only from a top-level `==` outside every `||` arm. A disjunction, an
/// inequality whose operand holds a closure `==`, or an `==` inside a call
/// argument pins nothing. `assert!` already reads these weak (control).
#[test]
fn ensure_disjunctions_and_nested_equalities_are_never_exact() -> Result<(), String> {
    let weak = (OracleKind::RelationalCheck, OracleStrength::Weak);
    let exact = (OracleKind::ExactValue, OracleStrength::Strong);
    let err = (OracleKind::ExactErrorVariant, OracleStrength::Strong);
    for (text, expected) in [
        ("ensure!(a != b || c == d);", &weak),
        ("ensure!(a == b || c == d);", &weak),
        ("ensure!(xs.iter().position(|x| *x == 1) != None);", &weak),
        ("ensure!(f(|x| x == 1));", &weak),
        (
            "ensure!(r == Err(Error::Denied) || r == Err(Error::Locked));",
            &weak,
        ),
        ("assert!(a != b || c == d);", &weak),
        // An observer in an `||` arm is optional, so it earns no observer
        // credit; joined by `&&` it keeps its medium reading.
        ("assert!(ready || events_sent.contains(&id));", &weak),
        ("assert!(ready || !events.is_empty());", &weak),
        (
            "assert!(ready || { insta::assert_snapshot!(value); true });",
            &weak,
        ),
        ("assert!(ready && (done || mock.was_called()));", &weak),
        ("assert!(xs.iter().position(|x| *x == 1) != None);", &weak),
        // A deciding `||`, top-level or in a parenthesized conjunct, gates
        // the `matches!` and error-variant readings too.
        (
            "ensure!(matches!(r, Err(Error::Denied)) || matches!(r, Err(Error::Locked)));",
            &weak,
        ),
        (
            "ensure!(ready && (matches!(r, Err(Error::Denied)) || matches!(r, Err(Error::Locked))));",
            &weak,
        ),
        ("ensure!((matches!(r, Ok(3)) || fallback));", &weak),
        ("ensure!(ready && (a == b || c));", &weak),
        (
            "assert!(matches!(r, Err(Error::Denied)) || matches!(r, Err(Error::Locked)));",
            &weak,
        ),
        ("assert!(matches!(r, Ok(3)) || fallback);", &weak),
        ("assert!(ready && (matches!(r, Ok(3)) || fallback));", &weak),
        ("ensure!(a == b);", &exact),
        (
            "assert!(ready && events_sent.contains(&id));",
            &(OracleKind::MockExpectation, OracleStrength::Medium),
        ),
        ("ensure!(a == b, \"total must match\");", &exact),
        ("ensure!(a != b && c == d);", &exact),
        ("ensure!((total(&cart) == 42) && ready);", &exact),
        // Controls: an `||` inside a call or closure operand decides nothing.
        ("assert!(matches!(r, Ok(3)));", &exact),
        (
            "assert!(matches!(pick(|x| x > 1 || x < 0), Ok(3)));",
            &exact,
        ),
        ("ensure!(matches!(r, Err(Error::Denied)) && ready);", &err),
        (
            "ensure!(matches!(pick(|x| x > 1 || x < 0), Ok(3)));",
            &exact,
        ),
        // A parenthesized `||` conjunct drops out and the other conjuncts
        // decide: a required `==` or pattern beside it still pins.
        ("ensure!(c == d && (a || b));", &exact),
        ("ensure!((a || b) && c == d);", &exact),
        ("assert!(matches!(r, Ok(3)) && (a || b));", &exact),
        ("assert!((a || b) && matches!(r, Ok(3)));", &exact),
        (
            "ensure!(matches!(r, Err(Error::Denied)) && (x || y));",
            &err,
        ),
        ("ensure!(ready && /* fallback */ (a == b || c));", &weak),
        // A leading comment cannot hide the group from the rest step.
        ("assert!(matches!(r, Ok(3)) && /* c */ (a || b));", &exact),
        // A deciding group drops out before the negated-pattern and
        // compared-pattern caps, so a pinning sibling still decides.
        (
            "assert!(matches!(r, Ok(3)) && (!matches!(r, Err(E::X)) || y));",
            &exact,
        ),
        ("ensure!(c == d && (!matches!(r, Err(E::X)) || y));", &exact),
        // An `assert_eq!` inside an `||` arm lends no whole-object kind.
        (
            "assert!(flag || helper(|| assert_eq!(x, Foo { a: 1 })));",
            &weak,
        ),
        // The rest keeps its own reading and is never lifted to weak.
        (
            "assert!(opt.is_some() && (a == b || c));",
            &(OracleKind::SmokeOnly, OracleStrength::Smoke),
        ),
        // A disjunction with a broad error arm passes without an error.
        ("assert!(ready || r.is_err());", &weak),
        ("ensure!(ready || r.is_err());", &weak),
    ] {
        let classification = classify_assertion(text);
        if (&classification.kind, &classification.strength) != (&expected.0, &expected.1) {
            return Err(format!(
                "{text}: expected {:?}/{:?}, got {:?}/{:?}",
                expected.0, expected.1, classification.kind, classification.strength
            ));
        }
    }
    // A trailing line comment on a kept conjunct must not swallow the rest
    // or the closing `);` of the rewritten assertion.
    for (text, expected) in [
        (
            "assert!(\n    (a || b) &&\n    matches!(r, Ok(_)) // must succeed\n);",
            (OracleKind::SmokeOnly, OracleStrength::Smoke),
        ),
        (
            "assert!(\n    matches!(r, Ok(_)) // must succeed\n    && (a || b)\n);",
            (OracleKind::SmokeOnly, OracleStrength::Smoke),
        ),
        (
            "assert!(\n    !matches!(r, Err(E::X)) &&\n    matches!(r, Ok(_)) // ok\n);",
            (OracleKind::SmokeOnly, OracleStrength::Smoke),
        ),
    ] {
        let facts = extract_assertions(text, 7558);
        let Some(fact) = facts.first() else {
            return Err(format!("{text}: must produce an oracle fact"));
        };
        require_kind(fact, expected.0, expected.1)?;
    }
    // The inequality predicate reveal reads agrees with the cap order.
    let dropped = "assert!(matches!(r, Ok(3)) && (!matches!(r, Err(E::X)) || y));";
    if is_inequality_assertion(dropped) {
        return Err(format!(
            "{dropped}: a dropped group must not make it an inequality"
        ));
    }
    if !is_inequality_assertion("assert!(ready || !matches!(r, Err(E::X)));") {
        return Err("a negated pattern in an `||` arm stays an inequality".to_string());
    }
    let facts = extract_assertions("ensure!(a != b || c == d);", 7558);
    let Some(fact) = facts.first() else {
        return Err("the ensure! line must produce an oracle fact".to_string());
    };
    require_kind(fact, OracleKind::RelationalCheck, OracleStrength::Weak)
}

#[test]
fn a_matches_conjunct_reads_no_stronger_than_its_pattern() -> Result<(), String> {
    let smoke = (OracleKind::SmokeOnly, OracleStrength::Smoke);
    let weak = (OracleKind::RelationalCheck, OracleStrength::Weak);
    let exact = (OracleKind::ExactValue, OracleStrength::Strong);
    let err = (OracleKind::ExactErrorVariant, OracleStrength::Strong);
    let broad = (OracleKind::BroadError, OracleStrength::Weak);
    for (text, expected) in [
        // #7695: a side-only pattern in an `&&` chain keeps its reading; a
        // bare conjunct beside it reads weak (step 13), never strong.
        ("assert!(ready && matches!(r, Ok(_)));", &weak),
        ("assert!(matches!(r, Ok(_)) && ready);", &weak),
        ("debug_assert!(ready && matches!(r, Some(_)));", &weak),
        ("assert!(ready && (matches!(r, Ok(_))));", &weak),
        ("assert!(ready && /* side */ matches!(r, Ok(_)));", &weak),
        (
            "assert!(matches!(r, Ok(_)) && matches!(s, Some(_)));",
            &smoke,
        ),
        ("assert!(total > 1 && matches!(r, Ok(_)));", &weak),
        ("assert!(ready && matches!(r, Ok(_) | Err(_)));", &weak),
        ("assert!(ready && matches!(r, Some(1..=5)));", &weak),
        ("assert!(ready && matches!(r, Err(_)));", &broad),
        // Controls: a conjunct that pins keeps the chain's reading.
        ("assert!(ready && matches!(r, Ok(3)));", &exact),
        ("assert!(matches!(r, Ok(_)) && matches!(r, Ok(3)));", &exact),
        // A pinning `==` conjunct (rule 1) fixes a value, so the chain keeps
        // its reading rather than risk an actionable gap.
        ("assert!(total == 4 && matches!(r, Ok(_)));", &exact),
        ("assert!(matches!(value, _) && value == 2);", &exact),
        ("ensure!(total == 4 && matches!(r, Ok(_)));", &exact),
        ("assert!(ready && matches!(r, Err(Error::Denied)));", &err),
        ("assert!(matches!(r, Ok(_)));", &smoke),
        // `prop_assert!` reads its pattern like `assert!` does.
        ("prop_assert!(matches!(r, Ok(_)));", &smoke),
        ("prop_assert!(ready && matches!(r, Ok(_)));", &weak),
        ("prop_assert!(ready && matches!(r, Ok(3)));", &exact),
        // A self-comparison pins nothing, so it does not exempt the chain.
        ("assert!(value == value && matches!(r, Ok(_)));", &weak),
        ("ensure!(value == value && ready);", &weak),
    ] {
        require_classification(text, expected)?;
    }
    // The extracted fact agrees, including when a trailing line comment on a
    // conjunct could swallow the rewritten `);` and leave the chain unread.
    for (text, expected) in [
        ("assert!(ready && matches!(r, Ok(_)));", &weak),
        ("assert!(ready && matches!(r, Ok(3)));", &exact),
        ("assert!(total == 4 && matches!(r, Ok(_)));", &exact),
        (
            "assert!(\n    ready // set above\n    && matches!(r, Ok(_))\n);",
            &weak,
        ),
        (
            "assert!(\n    ready &&\n    matches!(r, Ok(_)) // side only\n);",
            &weak,
        ),
    ] {
        let facts = extract_assertions(text, 7695);
        let Some(fact) = facts.first() else {
            return Err(format!("{text}: must produce an oracle fact"));
        };
        require_kind(fact, expected.0.clone(), expected.1.clone())?;
    }
    Ok(())
}

#[test]
fn compound_matches_forms_read_no_stronger_than_their_pattern() -> Result<(), String> {
    let weak = (OracleKind::RelationalCheck, OracleStrength::Weak);
    let exact = (OracleKind::ExactValue, OracleStrength::Strong);
    for (text, expected) in [
        // #7715: a `matches!` nested in a closure earns nothing from its text alone.
        (
            "assert!(ready && matches!(r, Ok(_)) && xs.iter().any(|x| matches!(x, Some(_))));",
            &weak,
        ),
        ("assert!(xs.iter().any(|x| matches!(x, Some(_))));", &weak),
        // A spaced or commented std path is still the std pattern test.
        (
            "assert!(xs.iter().any(|x| std :: matches!(x, Some(_))));",
            &weak,
        ),
        (
            "assert!(xs.iter().any(|x| core /* c */ :: matches!(x, Some(_))));",
            &weak,
        ),
        // `matches!` named only in a comment or string is no pattern.
        (
            "assert!(ready /* matches! */ && matches!(r, Ok(_)));",
            &weak,
        ),
        ("assert!(ready, \"matches!(r, Ok(3))\");", &weak),
        // Comparing an unpinned pattern with a bool pins nothing.
        ("assert!(matches!(r, Ok(_)) == true);", &weak),
        ("ensure!(matches!(r, Ok(_)) == true);", &weak),
        ("assert!(matches!(r, _) == true);", &weak),
        ("assert!((matches!(r, Ok(_))) == true);", &weak),
        ("assert!(matches!(r, Ok(_)) == (true));", &weak),
        ("assert!(false == matches!(r, Ok(_)));", &weak),
        // A pinned pattern compared with `false` asserts it does not match,
        // which pins nothing either (rule 1).
        ("assert!(matches!(r, Ok(3)) == false);", &weak),
        (
            "assert!(xs.iter().any(|x| ::core::matches!(x, Some(_))));",
            &weak,
        ),
        // #7772: a comment before the constructor never hides it.
        (
            "assert!(xs.iter().any(|x| matches!(x, /* , */ Some(_))));",
            &weak,
        ),
        ("ensure!(matches!(r, /* , */ Ok(_)) == true);", &weak),
        (
            "assert!(xs.iter().any(|x| matches!(x, // ,\n Some(_))));",
            &weak,
        ),
        ("ensure!(matches!(r, // ,\n Ok(_)) == true);", &weak),
        (
            "assert!(xs.iter().any(|x| matches!(x, /* , /* , */ */ Some(_))));",
            &weak,
        ),
        (
            "ensure!(matches!(r, /* , /* , */ */ Ok(_)) == true);",
            &weak,
        ),
        (
            "assert!(xs.iter().any(|x| matches!(x, Some /* c */ (_))));",
            &weak,
        ),
        // Controls: a pattern that pins keeps exact credit in every form.
        ("assert!(xs.iter().any(|x| matches!(x, Some(3))));", &exact),
        (
            "assert!(xs.iter().any(|x| matches!(x, /* , */ Some(3))));",
            &exact,
        ),
        ("ensure!(matches!(r, /* , */ Ok(3)) == true);", &exact),
        (
            "assert!(xs.iter().any(|x| matches!(x, // ,\n Some(3))));",
            &exact,
        ),
        (
            "ensure!(matches!(r, /* , /* , */ */ Ok(3)) == true);",
            &exact,
        ),
        // #7822: brackets inside a nested comment must not hide a pinned
        // pattern compared with a bool. The same comment in a closure
        // already keeps exact credit; the bool-comparison path must too.
        // Unpinned patterns with that comment stay weak: the fix only
        // restores polarity, it does not promote an unpinned match.
        (
            "ensure!(matches!(r, /* , ( [ /* , ) ] */ } */ Ok(_)) == true);",
            &weak,
        ),
        (
            "assert!(xs.iter().any(|x| matches!(x, /* , ( [ /* , ) ] */ } */ Some(_))));",
            &weak,
        ),
        (
            "assert!(matches!(r, /* , ( [ /* , ) ] */ } */ Ok(3)) == false);",
            &weak,
        ),
        (
            "ensure!(matches!(r, /* , ( [ /* , ) ] */ } */ Ok(3)) == true);",
            &exact,
        ),
        (
            "assert!(xs.iter().any(|x| matches!(x, /* , ( [ /* , ) ] */ } */ Some(3))));",
            &exact,
        ),
        (
            "assert!(matches!(r, /* , ( [ /* , ) ] */ } */ Ok(3)) == true);",
            &exact,
        ),
        (
            "assert!(true == matches!(r, /* , ( [ /* , ) ] */ } */ Ok(3)));",
            &exact,
        ),
        (
            "assert!((matches!(r, /* , ( [ /* , ) ] */ } */ Ok(3))) == true);",
            &exact,
        ),
        (
            "ensure!(matches!(r, // , ( [ ) ] }\n Ok(3)) == true);",
            &exact,
        ),
        (
            "ensure!(matches!(r, // , ( [ ) ] }\n Ok(_)) == true);",
            &weak,
        ),
        // Rule 1 already reads a compared pattern nested in a chain as not
        // positive, whatever its pattern.
        ("assert!(ready && matches!(r, Ok(3)) == true);", &weak),
        ("ensure!(matches!(r, Ok(3)) == true);", &exact),
        ("assert!(matches!(r, Ok(3)) == true);", &exact),
        ("assert!(total == 4 && matches!(r, Ok(_)));", &exact),
        ("assert!(flag == true && matches!(r, Ok(_)));", &exact),
        // A user macro whose name ends in `matches!` keeps today's reading.
        ("assert!(custom_matches!(r, Ok(_)));", &exact),
        ("expect_matches!(r, Ok(3));", &exact),
        (
            "assert!(xs.iter().any(|x| custom::matches!(x, Some(_))));",
            &exact,
        ),
        (
            "assert!(xs.iter().any(|x| helpers::std::matches!(x, Some(_))));",
            &exact,
        ),
        ("assert!((matches!(r, Ok(3))) == true);", &exact),
    ] {
        require_classification(text, expected)?;
    }
    // A non-ASCII token before the macro path is read on a char boundary.
    let _ = classify_assertion("assert!(ok→std::matches!(r, Ok(_)));");
    Ok(())
}

/// #7598, RIPR-SPEC-0231 rule 1: a boolean-valued operand read through a
/// boolean literal, a negation or an operator the classifier does not read
/// states only a relation, so it never earns an exact kind.
#[test]
fn bool_valued_operands_read_as_what_they_assert() -> Result<(), String> {
    let weak = (OracleKind::RelationalCheck, OracleStrength::Weak);
    let exact = (OracleKind::ExactValue, OracleStrength::Strong);
    let err = (OracleKind::ExactErrorVariant, OracleStrength::Strong);
    for (text, expected) in [
        // A comparison asserted false is the inequality it states.
        ("assert_eq!(r == Err(E::Bad), false);", &weak),
        ("assert_eq!(false, r == Err(E::Bad));", &weak),
        ("debug_assert_eq!(r == Err(E::Bad), false);", &weak),
        ("prop_assert_eq!(r == Err(E::Bad), false);", &weak),
        ("assert_ne!(r == Err(E::Bad), true);", &weak),
        ("assert_ne!(true, r == Err(E::Bad));", &weak),
        // `assert_ne!` stays an inequality even where its bool side pins
        // the comparison; the cap only ever lowers.
        ("assert_ne!(r == Err(E::Bad), false);", &weak),
        ("assert_eq!(!(r == Err(E::Bad)), true);", &weak),
        ("assert_eq!((r == Err(E::Bad)).not(), true);", &weak),
        ("ensure!((a == b) == false);", &weak),
        ("ensure!(false == (a == b));", &weak),
        ("ensure!((a == b) != true);", &weak),
        // `!=` and spaced `<`, `>`, `<=`, `>=` asserted true are relations.
        ("assert_eq!(true, check(20) != Err(E::Bad));", &weak),
        ("assert_eq!(score(2) != 0, true);", &weak),
        ("assert_eq!(score(2) > 0, true);", &weak),
        ("assert_eq!(score(2) <= 9, true);", &weak),
        // A deciding `||` passes whenever one arm does.
        ("assert!((matches!(r, Ok(3)) || f) == true);", &weak),
        ("assert!((matches!(r, Err(E::A)) || f) == true);", &weak),
        ("ensure!((matches!(r, Ok(3)) || f) == true);", &weak),
        ("ensure!((a == b || c) == true);", &weak),
        ("assert_eq!(matches!(r, Ok(3)) || f, true);", &weak),
        ("assert_eq!(r == Ok(3) || f, true);", &weak),
        // A compared chain reads like the same chain in `assert!` (rule 3).
        ("assert_eq!(ready && matches!(r, Ok(_)), true);", &weak),
        ("assert_eq!(ready && matches!(r, Ok(3)), true);", &exact),
        // A pattern under an unread operator, call or `if` may be inverted.
        ("assert!(matches!(r, Err(E::Bad)) ^ true);", &weak),
        ("assert!(matches!(r, Err(E::Bad)) < true);", &weak),
        ("assert!(true > matches!(r, Err(E::Bad)));", &weak),
        (
            "assert!(std::ops::Not::not(matches!(r, Err(E::Bad))));",
            &weak,
        ),
        ("assert!(check((matches!(r, Err(E::Bad)))));", &weak),
        (
            "assert!(if matches!(r, Err(E::Bad)) { false } else { true });",
            &weak,
        ),
        ("assert!(check::<bool>(matches!(r, Err(E::Bad))));", &weak),
        ("assert!(make()(matches!(r, Err(E::Bad))));", &weak),
        ("assert!(checks[0](matches!(r, Err(E::Bad))));", &weak),
        // A non-ASCII name before the call is still read on a char boundary.
        ("assert!(prüfe(matches!(r, Err(E::Bad))));", &weak),
        // An unspaced comparison is still a relation.
        ("assert_eq!(score(2)>0, true);", &weak),
        ("assert_eq!(true, 0<score(2));", &weak),
        ("assert_eq!(Vec::<Vec<u8>>::new().len() > 0, true);", &weak),
        // `^` and `|` over a pattern or comparison may invert it, in either
        // assertion form, and so may a call taking it at any position.
        ("assert_eq!(matches!(r, Ok(3)) ^ true, true);", &weak),
        ("assert_eq!((r == Err(E::Bad)) ^ true, true);", &weak),
        ("assert_eq!(f(matches!(r, Err(E::Bad))), true);", &weak),
        ("assert!(f(1, matches!(r, Err(E::Bad))));", &weak),
        ("assert!(f(g(matches!(r, Err(E::Bad)))));", &weak),
        ("assert!(matches!(r, Err(E::Bad)) | true);", &weak),
        ("assert!(ready | matches!(r, Err(E::Bad)));", &weak),
        ("assert!(ready | (matches!(r, Err(E::Bad))));", &weak),
        ("ensure!(ready | matches!(r, Ok(3)) && score == 4);", &exact),
        (
            "assert_eq!(matches!(r, Err(E::Bad)) as u8 == 0, true);",
            &weak,
        ),
        ("assert!(matches!(r, Err(E::Bad)) as u8 == 1);", &weak),
        // A closure parameter list is not a `|` operator.
        ("assert!(xs.iter().all(|x| matches!(x, Some(3))));", &exact),
        (
            "assert!(xs.iter().all(move |x| matches!(x, Some(3))));",
            &exact,
        ),
        // A string or char literal before `<` is an operand, not a path.
        ("assert_eq!(\"x+\" < s, true);", &weak),
        ("assert_eq!('+' < c, true);", &weak),
        ("assert_eq!(s + \"x\" < t, true);", &weak),
        // A name that starts with `move` is not a closure.
        (
            "assert_eq!(moved_ok(matches!(r, Ok(3))) ^ true, true);",
            &weak,
        ),
        ("assert_eq!(moves(r == Err(E::Bad)) ^ flag, true);", &weak),
        // With no pinning `==` conjunct, a `!=` conjunct states a relation,
        // whether joined by `&&` or a single `&`.
        ("assert_eq!(score(2) != 0 && flag, true);", &weak),
        ("assert_eq!((r != Err(E::Bad)) && flag, true);", &weak),
        ("assert_eq!((r != Err(E::Bad)) & flag, true);", &weak),
        ("assert_eq!((score(2) != 0) & flag, true);", &weak),
        ("assert_eq!(ready & matches!(r, Ok(_)), true);", &weak),
        // So does a negated or false-compared conjunct.
        (
            "assert_eq!(flag && !matches!(r, Err(E::Bad)), true);",
            &weak,
        ),
        ("assert_eq!(flag & !matches!(r, Err(E::Bad)), true);", &weak),
        ("assert_eq!(flag && !(r == Err(E::Bad)), true);", &weak),
        (
            "assert_eq!(flag && (r == Err(E::Bad)) == false, true);",
            &weak,
        ),
        ("assert_eq!(flag && !(a == b), true);", &weak),
        ("assert_eq!(flag && !(score(2) < 3), true);", &weak),
        (
            "assert_eq!(flag && (r != Err(E::Bad)) == true, true);",
            &weak,
        ),
        ("assert_eq!((r != Err(E::Bad)) == true, true);", &weak),
        ("assert_eq!(flag && (score(2) < 3) == true, true);", &weak),
        // A bool value compared with `false` pins it, like `!x`.
        ("assert_eq!(flag && x == false, true);", &exact),
        ("assert_eq!(x == false, true);", &exact),
        ("assert_eq!(a == b && c != d, true);", &exact),
        // A leading `!` binds to its own operand, not the whole chain.
        ("assert_eq!(!flag && matches!(r, Ok(3)), true);", &exact),
        // A single `&` over booleans is a conjunction: both must hold.
        ("assert!(matches!(r, Err(E::Bad)) & ready);", &err),
        (
            "assert_eq!(xs.iter().any(|x| *x == 3) & flag, true);",
            &exact,
        ),
        // An `if` anywhere in the condition may invert the pattern it tests.
        (
            "assert!(flag == if g(S { x: 1 }) && matches!(r, Err(E::Bad)) { false } else { true });",
            &weak,
        ),
        // A guard in an earlier `matches!` and a raw identifier are not
        // conditions around the pattern.
        (
            "assert!(matches!(x, Some(v) if v > 0) && matches!(r, Err(E::Bad)));",
            &err,
        ),
        ("assert!(r#match && matches!(r, Err(E::Bad)));", &err),
        (
            "assert!(flag == if ready && matches!(r, Ok(3)) { false } else { true });",
            &weak,
        ),
        (
            "assert!(flag == if matches!(r, Ok(3)) { false } else { true });",
            &weak,
        ),
        (
            "assert!(match r { Ok(_) => matches!(s, Ok(3)), _ => false });",
            &weak,
        ),
        // A compared chain in a whole condition keeps the compared-pattern
        // cap; the chain reading never raises it.
        ("assert!((ready && matches!(r, Ok(3))) == true);", &weak),
        (
            "assert_eq!((flag && matches!(r, Ok(3))) == true, true);",
            &weak,
        ),
        (
            "assert_eq!((flag && matches!(r, Ok(3))) != false, true);",
            &weak,
        ),
        (
            "assert_eq!(!((flag && matches!(r, Ok(3))) == false), true);",
            &weak,
        ),
        (
            "assert_eq!(true == (flag && matches!(r, Ok(3))), true);",
            &weak,
        ),
        (
            "assert_eq!((matches!(r, Ok(3)) && flag).not() == false, true);",
            &weak,
        ),
        (
            "assert_eq!((flag && matches!(r, Err(E::Bad))) == true, true);",
            &weak,
        ),
        // The cap holds when the compared chain is itself one conjunct.
        (
            "assert_eq!(flag && ((ready && matches!(r, Ok(3))) == true), true);",
            &weak,
        ),
        (
            "assert!(flag && ((ready && matches!(r, Ok(3))) == true));",
            &weak,
        ),
        (
            "assert!(((ready && matches!(r, Ok(3))) == true) && flag);",
            &weak,
        ),
        // A bool value compared with a literal reads as that value or its
        // negation, which pins it.
        ("assert_eq!(x != true, true);", &exact),
        ("assert_eq!(is_valid(v) == true, false);", &exact),
        // The conjunct with the unread pattern drops out and the other
        // required conjuncts decide.
        ("ensure!(check(matches!(r, Ok(3))) && score == 4);", &exact),
        ("ensure!(score == 4 && check(matches!(r, Ok(3))));", &exact),
        (
            "assert!(check(matches!(r, Ok(3))) && matches!(s, Err(E::Bad)));",
            &err,
        ),
        ("ensure!(check(matches!(r, Ok(3))) && ready);", &weak),
        (
            "assert!(f(matches!(r, Ok(3))) && g(matches!(s, Ok(4))));",
            &weak,
        ),
        // In the equality-macro form the whole compared operand caps.
        (
            "assert_eq!(score == 4 && check(matches!(r, Ok(3))), true);",
            &weak,
        ),
        // A pattern under a call inside a closure guard is not an operand of
        // the asserted condition.
        (
            "assert!(xs.iter().filter(|x| f(matches!(x, Some(3)))).count() == 2);",
            &exact,
        ),
        // Controls: a comparison asserted true, a bare value or call, a
        // pinning chain and a pattern in a closure keep today's reading.
        ("assert_eq!(r == Err(E::Bad), true);", &err),
        // #7931 controls: an `eq` method or a literal spelling asserted true
        // pins like `==`, and a method after a call that takes the pattern
        // belongs to the call.
        ("assert_eq!(r.eq(&Err(E::Bad)), true);", &err),
        ("assert_eq!(r == Err(E::Bad), true as bool);", &err),
        ("assert_eq!({ r == Err(E::Bad) }, true);", &err),
        ("assert_eq!(score(2).eq(&6), bool::from(true));", &exact),
        ("assert_eq!((r == Err(E::Bad)).eq(&true), true);", &err),
        (
            "assert_eq!(std::cmp::PartialEq::eq(&r, &Err(E::Bad)), true);",
            &err,
        ),
        ("assert_eq!(flag.eq(&false), true);", &exact),
        // #7993 controls: a qualified path call asserted true pins like
        // `==`, and a doubly negated literal is the literal.
        (
            "assert_eq!(<_ as PartialEq>::eq(&r, &Err(E::Bad)), true);",
            &err,
        ),
        ("assert_eq!(Result::eq(&r, &Err(E::Bad)), !&false);", &err),
        ("assert_eq!(r == Err(E::Bad), !!&true);", &err),
        // A module named `eq` is not a comparison call.
        ("assert_eq!(flag ^ helpers::eq::value(), true);", &exact),
        // #8006 controls: a pattern in a closure or read call keeps its
        // reading, and a bool value compared with a non-literal pins it.
        (
            "assert!(xs.iter().any(|x| { matches!(x, Some(3)) }));",
            &exact,
        ),
        ("assert!(foo(matches!(x, A)).is_ok());", &exact),
        ("assert_eq!(is_valid(x), expected);", &exact),
        ("assert_eq!(xs.iter().any(|x| *x == 1), expected);", &exact),
        // A bitwise `&` over plain values pins a value (review of 49bc452).
        ("assert_eq!(flags & MASK, 0);", &exact),
        ("assert_eq!(flags & Flags::A, Flags::A);", &exact),
        // An integer cast or `From` of a value pins it (#8093), and a
        // negated literal that keeps a pattern method's polarity asserts
        // the pattern (#8081).
        ("assert_eq!(x as u8, 3);", &exact),
        ("assert_eq!(u32::from(c), 97);", &exact),
        ("assert_eq!(is_valid(x) as u8, 1);", &exact),
        ("assert!(matches!(r, Err(E::Bad)).eq(&!false));", &err),
        // A value compared inside a literal comparison still pins.
        ("assert_eq!((x as u8 == 3), true);", &exact),
        // A relation-only conjunct beside a pinning `==` lends no kind: the
        // pinning conjunct decides it (#7947).
        (
            "assert_eq!(a == b && !matches!(r, Err(E::Bad)), true);",
            &exact,
        ),
        ("assert_eq!(a == b && r != Err(E::Bad), true);", &exact),
        (
            "assert_eq!(!matches!(r, Err(E::Bad)) && a == b, true);",
            &exact,
        ),
        ("assert_eq!(r == Err(E::Bad) && score(2) != 0, true);", &err),
        // A dropped compared pattern still caps the whole (review of #7947).
        (
            "assert_eq!(a == b && matches!(r, Err(E::Bad)) == false, true);",
            &weak,
        ),
        (
            "assert_eq!(a == b && matches!(r, Err(E::Bad)) != true, true);",
            &weak,
        ),
        (
            "assert_eq!((a == b && !matches!(r, Err(E::Bad))) == true, true);",
            &weak,
        ),
        (
            "assert_eq!(a == b // note\n && r != Err(E::Bad), true);",
            &exact,
        ),
        // An `eq`/`ne` chain against literals that keeps a pattern's
        // polarity asserts the pattern (review of #7992).
        ("assert!(matches!(r, Err(E::Bad)).eq(&true));", &err),
        ("assert!(matches!(r, Err(E::Bad)).ne(&false));", &err),
        (
            "assert!((matches!(r, Err(E::Bad))).eq(&false).eq(&false));",
            &err,
        ),
        ("assert!((matches!(r, Err(E::Bad)).eq(&true)));", &err),
        ("assert_eq!(r == Err(E::Bad), (true as bool));", &err),
        ("assert_eq!(&(r == Err(E::Bad)) == &false, false);", &err),
        // A double negation through literal spellings asserts the
        // comparison itself, like `(r == Err(E::Bad)) == false` asserted
        // false.
        (
            "assert_eq!((r == Err(E::Bad)) == false as bool, false);",
            &err,
        ),
        (
            "assert!(xs.iter().filter(|x| matches!(x, Some(3))).count() == 2);",
            &exact,
        ),
        ("assert_eq!(true, score(2) == 6);", &exact),
        ("assert_eq!(!is_valid(x), true);", &exact),
        ("assert_eq!(is_valid(x), false);", &exact),
        ("assert_eq!(is_a(x) && is_b(x), true);", &exact),
        ("assert_eq!(a == b && c == d, true);", &exact),
        ("assert_eq!(Vec::<u8>::new() == v, true);", &exact),
        ("assert_eq!(Vec::<u8 >::new() == v, true);", &exact),
        ("assert_eq!(Vec::<Vec<u8>>::new() == v, true);", &exact),
        // A leading or qualified `<` opens a path, not a comparison.
        ("assert_eq!(<Vec<u8>>::new() == v, true);", &exact),
        ("assert_eq!(<T as Tr>::f(x) == y, true);", &exact),
        ("assert_eq!(foo::<fn() -> u8>(x) == y, true);", &exact),
        ("assert_eq!(a << 2 == b, true);", &exact),
        ("assert_eq!(a & mask == b, true);", &exact),
        ("assert_eq!(is_a(x) ^ true, true);", &exact),
        // A call whose result is read again, a nested pattern scrutinee and a
        // closure body are not unread operands.
        (
            "assert!(matches!(build(matches!(k, Kind::A)), Ok(Mode::Dark)));",
            &exact,
        ),
        ("assert!(foo(matches!(x, A)).is_ok());", &exact),
        ("assert!(Some(true) == Some(matches!(r, Ok(3))));", &exact),
        (
            "assert!(xs.iter().any(|x| f(matches!(x, Some(3)))));",
            &exact,
        ),
        ("assert!(iffy(x) && matches!(r, Ok(3)));", &exact),
        ("assert_eq!(score(2), 6);", &exact),
        ("assert!(matches!(r, Ok(3)) == true);", &exact),
        ("assert!((matches!(r, Ok(3))) == true);", &exact),
        ("assert!(xs.iter().any(|x| matches!(x, Some(3))));", &exact),
        (
            "assert!(xs.iter().any(|x| (matches!(x, Some(3)))));",
            &exact,
        ),
        ("assert!(ready && (matches!(r, Ok(3))));", &exact),
        ("ensure!(a == b);", &exact),
    ] {
        require_classification(text, expected)?;
    }
    for text in [
        "assert_eq!(r == Err(E::Bad), false);",
        "assert!(std::ops::Not::not(matches!(r, Err(E::Bad))));",
        "assert!(f(matches!(r, Ok(3))) && g(matches!(s, Ok(4))));",
        "assert_eq!(\"x+\" < s, true);",
        "assert_eq!((r != Err(E::Bad)) & flag, true);",
        "assert!(ready | matches!(r, Err(E::Bad)));",
        // #7931: method-call, combinator, cast, call and block spellings.
        "assert!(matches!(r, Err(E::Bad)).eq(&false));",
        "assert_eq!(r.eq(&Err(E::Bad)), false);",
        "assert!(matches!(r, Err(E::Bad)).then(|| false).unwrap_or(true));",
        "assert!((matches!(r, Err(E::Bad))).then(|| false).unwrap_or(true));",
        "assert_eq!(r == Err(E::Bad), false as bool);",
        "assert_eq!(r == Err(E::Bad), bool::from(false));",
        "assert_eq!({ r == Err(E::Bad) }, false);",
        "assert_eq!(r.ne(&Err(E::Bad)), true);",
        "assert_eq!(score(2).ne(&0), true);",
        "assert_eq!(score(2).lt(&3), true);",
        "assert_eq!(&(r == Err(E::Bad)), &false);",
        "assert_eq!((r == Err(E::Bad)).eq(&false), true);",
        "assert_eq!(r.eq(&Err(E::Bad)).eq(&false), true);",
        "assert_eq!(score(2).eq(&6).ne(&true), true);",
        "assert_eq!(PartialEq::eq(&r, &Err(E::Bad)), false);",
        "assert_eq!(std::cmp::PartialOrd::lt(&score(2), &3), true);",
        "assert_eq!((r == Err(E::Bad)) as bool, false);",
        "assert_eq!(bool::from(r == Err(E::Bad)), false);",
        "assert_eq!(flag && r.ne(&Err(E::Bad)), true);",
        // Review of #7992: grouped literal spellings, a trailing comma,
        // a trailing comment, and a receiver or path call after `&&`.
        "assert_eq!(r == Err(E::Bad), (false as bool));",
        "assert_eq!(r == Err(E::Bad), (bool::from(false)));",
        "assert_eq!(PartialEq::eq(&r, &Err(E::Bad),), false);",
        "assert_eq!(PartialEq::eq(&HashMap::<u8, u8>::new(), &m), false);",
        "assert_eq!(r.eq(&Err(E::Bad)) /* checked */, false);",
        "assert_eq!(flag && r.eq(&Err(E::Bad)).eq(&false), false);",
        "assert_eq!(flag && (r == Err(E::Bad)).eq(&false), false);",
        "assert_eq!(flag && PartialEq::eq(&(r == Err(E::Bad)), &false), false);",
        "assert_eq!(PartialEq::eq(&HashMap::<Vec<u8>, u8>::new(), &m), false);",
        // `^` and `|` over a comparison call, and a pattern method behind a
        // one-expression block or an odd literal-call chain.
        "assert_eq!(r.eq(&Err(E::Bad)) ^ true, true);",
        "assert_eq!(flag ^ r.ne(&Err(E::Bad)), true);",
        "assert_eq!(flag ^ r.eq(&Err(E::Bad)), false);",
        "assert_eq!(flag | score(2).lt(&3), true);",
        "assert_eq!(flag ^ PartialEq::ne(&r, &Err(E::Bad)), true);",
        "assert_eq!(ready && (flag ^ r.ne(&Err(E::Bad))), true);",
        "assert!(({ matches!(r, Err(E::Bad)) }).then(|| false).unwrap_or(true));",
        "assert!(matches!(r, Err(E::Bad)).eq(&false));",
        "assert!(matches!(r, Err(E::Bad)).ne(&true));",
        "assert!(matches!(r, Err(E::Bad)).eq(&true).then(|| false).unwrap_or(true));",
        "assert!(matches!(r, Err(E::Bad)).eq(&true) ^ flag);",
        // A polarity-keeping literal chain asserts the pattern only as the
        // whole condition (review of abe9f96).
        "assert_eq!(matches!(r, Err(E::Bad)).eq(&true), false);",
        "assert_eq!(!matches!(r, Err(E::Bad)).eq(&true), true);",
        "assert_eq!(false, matches!(r, Err(E::Bad)).ne(&false));",
        "assert!(std::ops::Not::not(matches!(r, Err(E::Bad)).eq(&true)));",
        "assert!(check(matches!(r, Err(E::Bad)).eq(&true)));",
        "assert!(!(flag || matches!(r, Err(E::Bad)).eq(&true)));",
        "assert!(!(flag && matches!(r, Err(E::Bad)).eq(&true)));",
        "assert!((matches!(r, Err(E::Bad)).eq(&true)) ^ flag);",
        "assert_eq!(PartialEq::eq(&f::<{ A < B }>(), &x), false);",
        "assert_eq!(/* c */ matches!(r, Err(E::Bad)).eq(&true), false);",
        "assert_eq!(matches!(r, Err(E::Bad)) /* c */ .eq(&true), false);",
        "assert_eq!((matches!(r, Err(E::Bad)) /* c */).eq(&true), false);",
        "assert_eq!(matches!(matches!(r, Err(E::Bad)), true).eq(&true), false);",
        // #7993: qualified-self and type-path calls, and a negated literal
        // spelling.
        "assert_eq!(<_ as PartialEq>::eq(&r, &Err(E::Bad)), false);",
        "assert_eq!(Result::eq(&r, &Err(E::Bad)), false);",
        "assert_eq!(Result::<u8, E>::eq(&r, &Err(E::Bad)), false);",
        "assert_eq!(<Result<u8, E> as PartialEq>::ne(&r, &Err(E::Bad)), true);",
        "assert_eq!(i32::lt(&score(2), &3), true);",
        "assert_eq!(r == Err(E::Bad), !&true);",
        "assert_eq!(r == Err(E::Bad), &!true);",
        "assert_eq!(r == Err(E::Bad), !{ true });",
        "assert_eq!(!&true, Result::eq(&r, &Err(E::Bad)));",
        "assert_eq!(flag ^ Result::ne(&r, &Err(E::Bad)), true);",
        "assert_eq!(Result ::ne(&r, &Err(E::Bad)), true);",
        "assert_eq!(<_ as PartialEq> :: eq(&r, &Err(E::Bad)), false);",
        "assert_eq!(C::<{ 2 }>::ne(&r, &Err(E::Bad)), true);",
        "assert_eq!(C::<{ A > B }>::eq(&r, &Err(E::Bad)), false);",
        "assert_eq!(Result::eq(&r, &Err(E::Bad)) ^ true, true);",
        "assert_eq!(flag | <_ as PartialEq>::ne(&r, &Err(E::Bad)), true);",
        "assert_eq!(matches!(r, Err(E::Bad)).eq(&!true), true);",
        // #8006: a negation of the whole condition through a tuple, array,
        // `Option`, block or group; a block under `^` or `|`; and a test
        // compared with a side that is no boolean literal.
        "assert!((matches!(r, Err(E::Bad)),).0.eq(&false));",
        "assert!([matches!(r, Err(E::Bad))][0].eq(&false));",
        "assert!(Some(matches!(r, Err(E::Bad))).unwrap().not());",
        "assert!(!{ matches!(r, Err(E::Bad)) });",
        "assert!(!(flag || matches!(r, Err(E::Bad))));",
        "assert!(!(flag && matches!(r, Err(E::Bad))));",
        "assert!(flag ^ { matches!(r, Err(E::Bad)) });",
        "assert!(flag | { matches!(r, Err(E::Bad)) });",
        "assert_eq!(matches!(r, Err(E::Bad)).eq(&true), expected);",
        "assert_eq!(!matches!(r, Err(E::Bad)).eq(&true), expected);",
        "assert_eq!(r == Err(E::Bad), expected);",
        "assert!(expected == (r == Err(E::Bad)));",
        "assert_eq!((r == Err(E::Bad)) ^ flag, expected);",
        "assert_eq!(flag | (r == Err(E::Bad)), expected);",
        "assert!((r == Err(E::Bad)) == expected);",
        "assert_eq!(matches!(r, Err(E::Bad)) & flag, expected);",
        "assert_eq!(flag & (r == Err(E::Bad)), expected);",
        // #8093: a test carried as an integer through a cast or `From`,
        // compared with a non-literal or with `0`.
        "assert_eq!((r == Err(E::Bad)) as u8, n);",
        "assert_eq!(u8::from(r == Err(E::Bad)), n);",
        "assert_eq!((r == Err(E::Bad)) as u8, 0);",
        "assert_eq!(n, (r == Err(E::Bad)) as i32);",
        "assert_eq!(i64::from(matches!(r, Err(E::Bad))), expected);",
        // #8101: a path call to `Not::not` over a whole condition, and a
        // negated literal that flips a pattern method (#8081).
        "assert!(Not::not(flag || matches!(r, Err(E::Bad))));",
        "assert!(std::ops::Not::not(flag && matches!(r, Err(E::Bad))));",
        "assert!(<bool as Not>::not(flag || matches!(r, Err(E::Bad))));",
        "assert!(matches!(r, Err(E::Bad)).ne(&!false));",
        // Review of 7830f13: `Not::not` over a comparison, and a test
        // compared with a non-literal inside a literal comparison.
        "assert_eq!(Not::not(r == Err(E::Bad)), true);",
        "assert_eq!((r == Err(E::Bad)) as u8 == 0, true);",
        "assert_eq!(u8::from(r == Err(E::Bad)) == 0, true);",
        "assert_eq!((r == Err(E::Bad)) == flag, true);",
        "assert!(((r == Err(E::Bad)) == flag) == true);",
        // Any path call named `not` reads as a negation (review of 1ae0c07).
        "assert_eq!(MyNot::not(r == Err(E::Bad)), true);",
        "assert_eq!(bool::not(r == Err(E::Bad)), true);",
        // #8101: `From`/`Into` conversion paths carry the test as an integer.
        "assert_eq!(From::from(r == Err(E::Bad)), n);",
        "assert_eq!(<u8 as From<bool>>::from(r == Err(E::Bad)), 0);",
        "assert_eq!(Into::<u8>::into(r == Err(E::Bad)), n);",
        "assert_eq!(std::primitive::u8::from(r == Err(E::Bad)), 0);",
    ] {
        if !is_inequality_assertion(text) {
            return Err(format!("{text}: must read as an inequality"));
        }
        let facts = extract_assertions(text, 7598);
        let Some(fact) = facts.first() else {
            return Err(format!("{text}: must produce an oracle fact"));
        };
        require_kind(fact, OracleKind::RelationalCheck, OracleStrength::Weak)?;
    }
    for text in [
        "assert_eq!(r == Err(E::Bad), true);",
        "ensure!(check(matches!(r, Ok(3))) && score == 4);",
        "assert!(matches!(build(matches!(k, Kind::A)), Ok(Mode::Dark)));",
        "assert!(foo(matches!(x, A)).is_ok());",
        "assert!(foo(|| { matches!(x, A) }).is_ok());",
        "assert!(matches!(r, Err(E::Bad)).eq(&true));",
        "assert!(foo::<T>(matches!(x, A)).is_ok());",
        "assert_eq!(flags | compute(|v| v == 1), EXPECTED);",
    ] {
        if is_inequality_assertion(text) {
            return Err(format!("{text}: must not read as an inequality"));
        }
    }
    // #8101: a turbofish call and a comparison nested inside a call's
    // arguments read as a pinned value, while a call that carries the test
    // through, a grouping parenthesis and a keyword's parenthesis still cap.
    for (text, expected) in [
        ("assert!(foo::<T>(matches!(x, A)).is_ok());", &exact),
        ("assert_eq!(flags | compute(|v| v == 1), EXPECTED);", &exact),
        ("assert_eq!(mask & lookup(a == b), X);", &exact),
        ("assert_eq!(flags | compute(v), EXPECTED);", &exact),
        ("assert_eq!(<u8 as From<bool>>::from(x), 1);", &exact),
        ("assert_eq!(Into::<u8>::into(c), 97);", &exact),
        (
            "assert_eq!(flag ^ bool::from(r == Err(E::Bad)), expected);",
            &weak,
        ),
        ("assert_eq!(flags | u8::from(a == b), x);", &weak),
        (
            "assert_eq!(flag ^ std::ops::Not::not(r == Err(E::Bad)), expected);",
            &weak,
        ),
        (
            "assert_eq!(flag & From::from(r == Err(E::Bad)), expected);",
            &weak,
        ),
        ("assert_eq!(flags | (a == b), X);", &weak),
        // Review of e2f22d1: a parenthesized type inside the conversion's
        // path, and a const-generic block or `->` inside a turbofish.
        (
            "assert_eq!(<u8 as From<(bool)>>::from(r == Err(E::Bad)), 0);",
            &weak,
        ),
        ("assert!(foo::<{ 2 > 1 }>(matches!(x, A)).is_ok());", &exact),
        (
            "assert!(foo::<fn() -> u8>(matches!(x, A)).is_ok());",
            &exact,
        ),
        ("assert_eq!(flag ^ Into::<u8>::into(a == b), x);", &weak),
        (
            "assert_eq!(flag ^ <bool as From<bool>>::from(a == b), x);",
            &weak,
        ),
        ("assert_eq!(flags | &mut (a == b), X);", &weak),
        // A multi-byte identifier character before a callee's last word
        // (review of e7a56e8: a byte offset past it is no char boundary).
        ("assert_eq!(flag ^ x\u{b7}y(a == b), x);", &exact),
        ("assert_eq!(flag ^ x\u{b7}not(a == b), x);", &exact),
        (
            "assert!(flag == if (ready && matches!(r, Ok(3))) { false } else { true });",
            &weak,
        ),
    ] {
        require_classification(text, expected)?;
    }
    for (text, plain) in [
        (
            "assert!(foo::<T>(matches!(x, A)).is_ok());",
            "assert!(foo(matches!(x, A)).is_ok());",
        ),
        (
            "assert_eq!(flags | compute(|v| v == 1), EXPECTED);",
            "assert_eq!(flags | compute(v), EXPECTED);",
        ),
        (
            "assert_eq!(mask & lookup(a == b), X);",
            "assert_eq!(mask & lookup(c), X);",
        ),
    ] {
        let plain = classify_assertion(plain);
        require_classification(text, &(plain.kind, plain.strength))?;
    }
    Ok(())
}

fn require_classification(
    text: &str,
    expected: &(OracleKind, OracleStrength),
) -> Result<(), String> {
    let classification = classify_assertion(text);
    if (&classification.kind, &classification.strength) != (&expected.0, &expected.1) {
        return Err(format!(
            "{text}: expected {:?}/{:?}, got {:?}/{:?}",
            expected.0, expected.1, classification.kind, classification.strength
        ));
    }
    Ok(())
}

// #8101: the same constructor around both sides compares what it wraps, so a
// test compared with a non-literal through `Some(..)` or `Ok(..)` caps like
// the bare comparison does.
#[test]
fn a_shared_constructor_does_not_hide_a_test_compared_with_a_non_literal() -> Result<(), String> {
    let weak = (OracleKind::RelationalCheck, OracleStrength::Weak);
    let exact = (OracleKind::ExactValue, OracleStrength::Strong);
    let variant = (OracleKind::ExactErrorVariant, OracleStrength::Strong);
    // The unwrapped comparisons are the baseline each wrapper must match.
    require_classification("assert_eq!(r == Err(E::Bad), expected);", &weak)?;
    require_classification("assert_eq!(r == Err(E::Bad), true);", &variant)?;
    for text in [
        "assert_eq!(Some(r == Err(E::Bad)), Some(expected));",
        "assert_eq!(Some(expected), Some(r == Err(E::Bad)));",
        "assert_eq!(Ok(r == Err(E::Bad)), Ok(expected));",
        "assert_eq!(Err(r == Err(E::Bad)), Err(expected));",
        "assert_eq!(Some(Some(r == Err(E::Bad))), Some(Some(expected)));",
        "assert_eq!(Some(matches!(r, Err(E::Bad))), Some(expected));",
    ] {
        require_classification(text, &weak)?;
    }
    // A wrapper around a literal compares like the literal does, and a
    // wrapped plain value still pins.
    require_classification("assert_eq!(Some(r == Err(E::Bad)), Some(true));", &variant)?;
    let plain = classify_assertion("assert_eq!(total(2), expected);");
    require_classification(
        "assert_eq!(Some(total(2)), Some(expected));",
        &(plain.kind, plain.strength),
    )?;
    require_classification("assert_eq!(Some(total(2)), Some(5));", &exact)?;
    Ok(())
}

// #8101: arithmetic over an integer-cast test and another non-literal term no
// longer says whether the test held, so it caps like the bare comparison.
#[test]
fn arithmetic_over_a_cast_test_does_not_pin_the_test() -> Result<(), String> {
    let weak = (OracleKind::RelationalCheck, OracleStrength::Weak);
    for text in [
        "assert_eq!(k * (r == Err(E::Bad)) as u32, n);",
        "assert_eq!(k + (r == Err(E::Bad)) as u32, n);",
        "assert_eq!((r == Err(E::Bad)) as u32 * k, n);",
        "assert_eq!(k * u32::from(r == Err(E::Bad)), n);",
        "assert_eq!(k * (r == Err(E::Bad)) as u32, 0);",
        "assert_eq!(k - u32::from(matches!(r, Err(E::Bad))), n);",
    ] {
        require_classification(text, &weak)?;
    }
    // Controls: arithmetic with no test, a call that merely contains a
    // comparison, and a value cast keep their readings.
    for text in [
        "assert_eq!(k * score(x) as u32, n);",
        "assert_eq!(total(a == b) + 1, n);",
        "assert_eq!(x as u32 + 1, 3);",
    ] {
        let plain = classify_assertion(text);
        if plain.strength == OracleStrength::Weak && plain.kind == OracleKind::RelationalCheck {
            return Err(format!("{text}: control capped"));
        }
    }
    Ok(())
}
