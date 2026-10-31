//! Pure text parsers behind the owner-pin operand readers. Each refusal
//! returns `None` so a spelling the parser cannot read exactly never earns a
//! pin; the literals below are independent of the parsers' own constants.

use super::*;

#[test]
fn unwrapped_call_reads_only_plain_unwrap_and_literal_expect() -> Result<(), String> {
    for (operand, expected) in [
        ("f(1).unwrap()", Some("f(1)")),
        ("  f(1) .unwrap()  ", Some("f(1)")),
        ("f(1).expect(\"boom\")", Some("f(1)")),
        ("f(1).expect( \"boom\" )", Some("f(1)")),
        ("f(1).expect(\"a)\")", Some("f(1)")),
        // A non-literal, escaped or quote-bearing message is not one plain literal.
        ("f(1).expect(message)", None),
        ("f(1).expect(\"a\\nb\")", None),
        ("f(1).expect(\"a\" \"b\")", None),
        // The error-side and bare forms never match the payload reader.
        ("f(1).unwrap_err()", None),
        ("f(1).expect_err(\"boom\")", None),
        ("f(1)", None),
        ("f(1).unwrap()?", None),
    ] {
        assert_eq!(unwrapped_call(operand), expected, "{operand}");
    }
    Ok(())
}

#[test]
fn unwrapped_err_call_reads_only_the_error_side_forms() -> Result<(), String> {
    for (operand, expected) in [
        ("f(1).unwrap_err()", Some("f(1)")),
        (" f(1) .unwrap_err() ", Some("f(1)")),
        ("f(1).expect_err(\"boom\")", Some("f(1)")),
        ("f(1).expect_err(message)", None),
        ("f(1).expect_err(\"a\\\"b\")", None),
        ("f(1).unwrap()", None),
        ("f(1).expect(\"boom\")", None),
        ("f(1)", None),
    ] {
        assert_eq!(unwrapped_err_call(operand), expected, "{operand}");
    }
    Ok(())
}

#[test]
fn claims_payload_call_requires_exactly_one_whole_macro_call() -> Result<(), String> {
    for (operand, expected) in [
        ("assert_ok!(f(1))", Some(("assert_ok", "f(1)"))),
        (
            "  assert_some! ( g(\")\") )  ",
            Some(("assert_some", "g(\")\")")),
        ),
        ("assert_some !(g)", Some(("assert_some", "g"))),
        // Nothing may follow the macro call.
        ("assert_ok!(f(1)).unwrap()", None),
        ("assert_ok!(f(1)) + 1", None),
        // Other macros, longer names, and non-paren delimiters are refused.
        ("assert_err!(f(1))", None),
        ("assert_okay!(f(1))", None),
        ("assert_ok![f(1)]", None),
        ("assert_ok(f(1))", None),
        ("assert_ok!(f(1)", None),
    ] {
        assert_eq!(claims_payload_call(operand), expected, "{operand}");
    }
    Ok(())
}

#[test]
fn path_call_prefix_refuses_type_and_non_identifier_segments() -> Result<(), String> {
    for (operand, expected) in [
        ("a::b::name(1)", Some("a::b")),
        ("crate::util::name(1)", Some("crate::util")),
        ("util::name(1)", Some("util")),
        ("name(1)", None),
        // A capitalized segment is a type, handled by the associated-call reader.
        ("Type::name(1)", None),
        ("a::Type::name(1)", None),
        // The callee must be exactly `name` followed by `(`.
        ("a::namex(1)", None),
        ("a::name", None),
        ("a b::name(1)", None),
    ] {
        assert_eq!(path_call_prefix(operand, "name"), expected, "{operand}");
    }
    Ok(())
}

#[test]
fn associated_call_type_needs_one_camel_case_non_self_head() -> Result<(), String> {
    for (operand, expected) in [
        ("Widget::new(1)", Some("Widget")),
        ("Widget::new(", Some("Widget")),
        ("Self::new(1)", None),
        ("widget::new(1)", None),
        ("a::Widget::new(1)", None),
        ("Widget::newer(1)", None),
        ("Widget::new", None),
        ("Wid get::new(1)", None),
    ] {
        assert_eq!(associated_call_type(operand, "new"), expected, "{operand}");
    }
    Ok(())
}

#[test]
fn unsafe_block_value_needs_a_single_expression_block() -> Result<(), String> {
    for (operand, expected) in [
        ("unsafe { f(1) }", Some("f(1)")),
        ("unsafe{f(1)}", Some("f(1)")),
        ("unsafe { f(\";\") }", Some("f(\";\")")),
        ("unsafe { /* SAFETY: x; y */ f(1) }", Some("f(1)")),
        // A statement, trailing text, an empty block, or a longer keyword refuses.
        ("unsafe { f(1); }", None),
        ("unsafe { let x = 1; f(x) }", None),
        ("unsafe { f(1) } + 1", None),
        ("unsafe {}", None),
        ("unsafe { }", None),
        ("unsafe_fn { f(1) }", None),
        ("unsafe f(1)", None),
        ("unsafe { f(1)", None),
    ] {
        assert_eq!(unsafe_block_value(operand), expected, "{operand}");
    }
    Ok(())
}

#[test]
fn constructed_receiver_needs_a_direct_method_call_after_one_constructor() -> Result<(), String> {
    for (operand, expected) in [
        ("Widget::new(1).read(2)", Some("Widget::new(1)")),
        ("Widget::new(a(')')).read()", Some("Widget::new(a(')'))")),
        ("Widget::new().read", None),
        ("Widget::new(1).reader()", None),
        ("Widget::new(1)", None),
        ("Widget::new.read()", None),
        ("Widget::(1).read()", None),
        ("widget::new(1).read()", None),
        ("Widget::new(1)?.read()", None),
        ("Widget::new(1.read()", None),
    ] {
        assert_eq!(constructed_receiver(operand, "read"), expected, "{operand}");
    }
    Ok(())
}

#[test]
fn constructor_call_span_is_the_close_of_exactly_one_call() -> Result<(), String> {
    for (text, expected) in [
        ("Err(x)", Some(5)),
        ("Err(x)  ", Some(5)),
        ("Err(\")\")", Some(7)),
        ("Err(a(b))", Some(8)),
        ("Err(x).y", None),
        ("Err(x)?", None),
        ("Errx(1)", None),
        ("Err (x)", None),
        ("Err(x", None),
        ("Ok(x)", None),
    ] {
        assert_eq!(constructor_call_span(text, "Err"), expected, "{text}");
    }
    Ok(())
}

#[test]
fn local_variant_path_names_only_local_two_segment_paths() -> Result<(), String> {
    let pair = |a: &str, b: &str| Some((a.to_string(), b.to_string()));
    for (path, expected) in [
        ("Type::Variant", pair("Type", "Variant")),
        ("crate::Type::Variant", pair("Type", "Variant")),
        (" Type :: Variant ", pair("Type", "Variant")),
        // Other prefixes may name a different enum sharing the trailing names.
        ("self::Type::Variant", None),
        ("super::Type::Variant", None),
        ("external::Type::Variant", None),
        ("::Type::Variant", None),
        ("a::b::Variant", None),
        ("Variant", None),
        ("Type::", None),
    ] {
        assert_eq!(local_variant_path(path), expected, "{path}");
    }
    Ok(())
}

#[test]
fn error_variant_key_reads_one_err_call_of_a_local_variant() -> Result<(), String> {
    let pair = |a: &str, b: &str| Some((a.to_string(), b.to_string()));
    for (text, expected) in [
        ("Err(Error::Bad)", pair("Error", "Bad")),
        (" Err( Error::Bad ) ", pair("Error", "Bad")),
        ("Err(Error::Bad(1))", pair("Error", "Bad")),
        ("Err(Error::Bad { code: 1 })", pair("Error", "Bad")),
        ("Err(crate::Error::Bad)", pair("Error", "Bad")),
        // Trailing operators, other delimiters and opaque errors refuse.
        ("Err(Error::Bad(1) + x)", None),
        ("Err(Error::Bad[1])", None),
        ("Err(Error::Bad(1)", None),
        ("Err(e)", None),
        ("Err(a::b::Bad)", None),
        ("Err(Error::Bad)?", None),
        ("Err(Error::Bad).x", None),
        ("Ok(Error::Bad)", None),
    ] {
        assert_eq!(error_variant_key(text), expected, "{text}");
    }
    Ok(())
}

#[test]
fn payload_variant_construction_requires_a_payload_shape() -> Result<(), String> {
    let pair = |a: &str, b: &str| Some((a.to_string(), b.to_string()));
    for (expression, expected) in [
        ("return Err(Error::Bad(1));", pair("Error", "Bad")),
        ("return  Err(Error::Bad(1)) ;", pair("Error", "Bad")),
        ("Err(Error::Bad { code: 1 })", pair("Error", "Bad")),
        // A unit variant carries no payload to pin.
        ("Err(Error::Bad)", None),
        ("return Err(Error::Bad);", None),
        ("Ok(Error::Bad(1))", None),
        ("return Err(e);", None),
    ] {
        assert_eq!(
            payload_variant_construction(expression),
            expected,
            "{expression}"
        );
    }
    Ok(())
}

#[test]
fn initialized_field_name_reads_a_plain_field_and_refuses_paths() -> Result<(), String> {
    for (expression, expected) in [
        ("count: 1", Some("count")),
        ("count : 1,", Some("count")),
        ("count,", Some("count")),
        ("count", Some("count")),
        ("r#type: 1", Some("type")),
        ("a::b", None),
        ("a::b: 1", None),
        ("1x: 3", None),
        ("0: 3", None),
        ("..base", None),
        ("a.b: 1", None),
        ("", None),
    ] {
        assert_eq!(initialized_field_name(expression), expected, "{expression}");
    }
    Ok(())
}

#[test]
fn starts_with_word_and_whole_word_count_respect_identifier_boundaries() -> Result<(), String> {
    for (text, expected) in [
        ("trait Foo", true),
        ("trait", true),
        ("trait(", true),
        ("traitor", false),
        ("trait_x", false),
        ("trait9", false),
        ("train", false),
        (" trait", false),
    ] {
        assert_eq!(starts_with_word(text, "trait"), expected, "{text}");
    }
    for (text, expected) in [
        ("f(1) + f(2)", 2),
        ("self.f(1) + f_x(2) + xf(3) + f", 2),
        ("f1 + f", 1),
        ("ff", 0),
        ("", 0),
    ] {
        assert_eq!(whole_word_count(text, "f"), expected, "{text}");
    }
    Ok(())
}

#[test]
fn spawn_join_body_reads_only_a_spawned_nullary_closure_joined_and_unwrapped() -> Result<(), String>
{
    for (text, expected) in [
        ("std::thread::spawn(|| f(1)).join().unwrap();", Some("f(1)")),
        (
            "std::thread::spawn(move || f(1)).join().unwrap()",
            Some("f(1)"),
        ),
        (
            "  std::thread::spawn ( move || f(1) ) . join ( ) . unwrap ( ) ; ",
            Some("f(1)"),
        ),
        (
            "std::thread::spawn(|| g(\")\")).join().unwrap()",
            Some("g(\")\")"),
        ),
        // The join result must be unwrapped and nothing may follow.
        ("std::thread::spawn(|| f(1)).join()", None),
        ("std::thread::spawn(|| f(1)).join().unwrap().x", None),
        ("std::thread::spawn(|| f(1)).join().expect(\"x\")", None),
        // A closure with parameters or an empty body, or another spawn, refuses.
        ("std::thread::spawn(|x| f(x)).join().unwrap()", None),
        ("std::thread::spawn(||).join().unwrap()", None),
        ("std::thread::spawn(f).join().unwrap()", None),
        ("std::thread::spawnx(|| f(1)).join().unwrap()", None),
        ("thread::spawn(|| f(1)).join().unwrap()", None),
    ] {
        assert_eq!(spawn_join_body(text).as_deref(), expected, "{text}");
    }
    Ok(())
}

#[test]
fn evaluates_conditionally_flags_short_circuit_and_control_flow_only() -> Result<(), String> {
    for (tail, expected) in [
        ("a + b * 2", false),
        ("a | b", false),
        ("iffy + 1", false),
        ("format_all(a)", false),
        ("a && b", true),
        ("a || b", true),
        ("if a { 1 } else { 2 }", true),
        ("match a { _ => 1 }", true),
        ("a + loop { break 1 }", true),
    ] {
        assert_eq!(evaluates_conditionally(tail), expected, "{tail}");
    }
    Ok(())
}
