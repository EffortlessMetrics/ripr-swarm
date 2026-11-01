//! Pins for the closed allow-list of non-comparison guard forms
//! (RIPR-SPEC-0260, "Understood guard forms"). Each form has a case that must
//! stop crediting `exposed` and a control that must keep crediting it when an
//! observed input enters the arm or the guard is not understood.

use super::classify::{PythonNoBehaviorContext, classify_change_with_context};
use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use std::path::Path;

const OWNER_FILE: &str = "src/shop.py";
const TEST_FILE: &str = "tests/test_shop.py";

/// Classify the last line of `owner` against `test_body`.
fn classify(owner: &str, old_line: &str, test_body: &str) -> Result<ExposureClass, String> {
    let line = owner.lines().count();
    let new_line = owner
        .lines()
        .nth(line - 1)
        .ok_or_else(|| "owner has no last line".to_string())?;
    let owners = extract_owners(Path::new(OWNER_FILE), owner);
    let tests = extract_tests(
        Path::new(TEST_FILE),
        &format!("from src.shop import label\n\n\n{test_body}"),
    );
    classify_change_with_context(
        Path::new(OWNER_FILE),
        line,
        new_line,
        Some(old_line),
        &owners,
        &tests,
        PythonNoBehaviorContext::default(),
        Some(owner),
    )
    .map(|finding| finding.class)
    .ok_or_else(|| "changed line should classify".to_string())
}

fn test_of(call: &str, expected: &str) -> String {
    format!("def test_label():\n    assert {call} == {expected}\n")
}

const OLD_NIL: &str = "        return 'none'";

#[test]
fn an_is_none_arm_no_observed_input_enters_does_not_credit() -> Result<(), String> {
    let arm = "def label(x):\n    if x is None:\n        return 'nil'\n";
    assert_eq!(
        classify(arm, OLD_NIL, &test_of("label(3)", "'3'"))?,
        ExposureClass::WeaklyExposed
    );
    // Positive control: an observed `None` takes the arm.
    assert_eq!(
        classify(arm, OLD_NIL, &test_of("label(None)", "'nil'"))?,
        ExposureClass::Exposed
    );
    // The unguarded fall-through keeps its credit.
    let owner = "def label(x):\n    if x is None:\n        return 'nil'\n    return str(x)\n";
    assert_eq!(
        classify(owner, "    return repr(x)", &test_of("label(3)", "'3'"))?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn an_is_not_none_arm_is_read_in_both_directions() -> Result<(), String> {
    let arm = "def label(x):\n    if x is not None:\n        return 'set'\n";
    assert_eq!(
        classify(arm, OLD_NIL, &test_of("label(None)", "None"))?,
        ExposureClass::WeaklyExposed
    );
    assert_eq!(
        classify(arm, OLD_NIL, &test_of("label(1)", "'set'"))?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_truthiness_guard_reads_a_defaulted_or_omitted_flag() -> Result<(), String> {
    let arm = "def label(x, flag=False):\n    if flag:\n        return 'loud'\n";
    // The test omits `flag`, so the default `False` leaves the arm untaken.
    assert_eq!(
        classify(arm, OLD_NIL, &test_of("label(1)", "None"))?,
        ExposureClass::WeaklyExposed
    );
    assert_eq!(
        classify(arm, OLD_NIL, &test_of("label(1, False)", "None"))?,
        ExposureClass::WeaklyExposed
    );
    assert_eq!(
        classify(arm, OLD_NIL, &test_of("label(1, True)", "'loud'"))?,
        ExposureClass::Exposed
    );
    Ok(())
}

/// Behavior pin for an entering parameter beside a same-named module constant
/// (#12171): omitted and explicit arguments, in both directions, through
/// `classify`. It is not the regression guard for the reader's precedence: the
/// owner extractor does not hand the reader the constant in this harness, so
/// the test stays green if that precedence is removed. The direct control,
/// `the_reader_prefers_the_call_row_over_a_constant_it_is_handed`, carries it.
#[test]
fn a_parameter_is_read_from_the_call_not_a_same_named_module_constant() -> Result<(), String> {
    // Constant `True`, parameter defaults to `False`: omitting the argument
    // leaves the arm untaken, so a constant that masked the parameter would
    // wrongly keep the credit.
    let constant_true =
        "flag = True\n\n\ndef label(x, flag=False):\n    if flag:\n        return 'loud'\n";
    assert_eq!(
        classify(constant_true, OLD_NIL, &test_of("label(1)", "None"))?,
        ExposureClass::WeaklyExposed
    );
    assert_eq!(
        classify(constant_true, OLD_NIL, &test_of("label(1, False)", "None"))?,
        ExposureClass::WeaklyExposed
    );
    assert_eq!(
        classify(constant_true, OLD_NIL, &test_of("label(1, True)", "'loud'"))?,
        ExposureClass::Exposed
    );
    // Constant `False`, argument `True`: the call enters the arm, so a constant
    // that masked the parameter would wrongly demote it.
    let constant_false =
        "flag = False\n\n\ndef label(x, flag=False):\n    if flag:\n        return 'loud'\n";
    assert_eq!(
        classify(
            constant_false,
            OLD_NIL,
            &test_of("label(1, True)", "'loud'")
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}

/// Direct control for the reader's precedence. The end-to-end test above goes
/// through the owner extractor, which may not hand the reader a same-named
/// constant at all, so it cannot tell whether `Reader::scalar` reads the
/// parameter first. Here the constant is passed to the reader explicitly.
#[test]
fn the_reader_prefers_the_call_row_over_a_constant_it_is_handed() -> Result<(), String> {
    use super::boundary::{Binding, CallRow};
    use super::guard_atoms::guard_condition_holds;
    use super::module_constants::PythonModuleConstant;
    use std::collections::BTreeMap;

    let source = "def label(x, flag=False):\n    if flag:\n        return 'loud'\n";
    let owners = extract_owners(Path::new(OWNER_FILE), source);
    let owner = owners
        .iter()
        .find(|owner| owner.name == "label")
        .ok_or_else(|| "label owner missing".to_string())?;
    let row_with = |value: &str| CallRow {
        file: Path::new(TEST_FILE).to_path_buf(),
        line: 1,
        text: "label(1)".to_string(),
        bindings: BTreeMap::from([(
            "flag".to_string(),
            Binding {
                value: value.to_string(),
                from_default: false,
            },
        )]),
        elements: BTreeMap::new(),
    };
    let constant = |value: &str| {
        vec![PythonModuleConstant {
            name: "flag".to_string(),
            value: value.to_string(),
            line: 1,
        }]
    };
    // The call enters the arm; a constant `False` must not decide otherwise.
    assert_eq!(
        guard_condition_holds(
            owner,
            "flag",
            1,
            &row_with("True"),
            &constant("False"),
            source
        ),
        Some(true)
    );
    // The call skips the arm; a constant `True` must not decide otherwise.
    assert_eq!(
        guard_condition_holds(
            owner,
            "flag",
            1,
            &row_with("False"),
            &constant("True"),
            source
        ),
        Some(false)
    );
    Ok(())
}

#[test]
fn a_negated_atom_and_a_string_or_number_truthiness_are_read() -> Result<(), String> {
    let negated = "def label(x, flag=True):\n    if not flag:\n        return 'quiet'\n";
    assert_eq!(
        classify(negated, OLD_NIL, &test_of("label(1)", "None"))?,
        ExposureClass::WeaklyExposed
    );
    assert_eq!(
        classify(negated, OLD_NIL, &test_of("label(1, False)", "'quiet'"))?,
        ExposureClass::Exposed
    );
    let text = "def label(flag):\n    if flag\x3a\n        return 'full'\n";
    assert_eq!(
        classify(text, OLD_NIL, &test_of("label('')", "None"))?,
        ExposureClass::WeaklyExposed
    );
    assert_eq!(
        classify(text, OLD_NIL, &test_of("label(0)", "None"))?,
        ExposureClass::WeaklyExposed
    );
    assert_eq!(
        classify(text, OLD_NIL, &test_of("label('a')", "'full'"))?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_literal_container_membership_guard_is_read() -> Result<(), String> {
    let arm = "def label(kind):\n    if kind in ('a', 'b'):\n        return 'known'\n";
    assert_eq!(
        classify(arm, OLD_NIL, &test_of("label('z')", "None"))?,
        ExposureClass::WeaklyExposed
    );
    assert_eq!(
        classify(arm, OLD_NIL, &test_of("label('a')", "'known'"))?,
        ExposureClass::Exposed
    );
    let not_in = "def label(kind):\n    if kind not in ['a']:\n        return 'other'\n";
    assert_eq!(
        classify(not_in, OLD_NIL, &test_of("label('a')", "None"))?,
        ExposureClass::WeaklyExposed
    );
    // A parenthesized scalar is not a container, so the guard is unreadable.
    let scalar = "def label(kind):\n    if kind in ('a'):\n        return 'known'\n";
    assert_eq!(
        classify(scalar, OLD_NIL, &test_of("label('z')", "None"))?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn an_isinstance_guard_is_read_for_builtin_classes() -> Result<(), String> {
    let arm = "def label(x):\n    if isinstance(x, str):\n        return 'text'\n";
    assert_eq!(
        classify(arm, OLD_NIL, &test_of("label(3)", "None"))?,
        ExposureClass::WeaklyExposed
    );
    assert_eq!(
        classify(arm, OLD_NIL, &test_of("label('s')", "'text'"))?,
        ExposureClass::Exposed
    );
    // `bool` is an `int`: `isinstance(True, int)` holds.
    let int_arm = "def label(x):\n    if isinstance(x, (int, float)):\n        return 'num'\n";
    assert_eq!(
        classify(int_arm, OLD_NIL, &test_of("label(True)", "'num'"))?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_shadowed_isinstance_is_not_read_as_the_builtin() -> Result<(), String> {
    // The owner's own `isinstance` parameter is what runs, so the builtin
    // reading of `isinstance(3, str)` must not demote the arm.
    let shadowed = "def label(x, isinstance):\n    if isinstance(x, str):\n        return 'text'\n";
    assert_eq!(
        classify(shadowed, OLD_NIL, &test_of("label(3, check)", "'text'"))?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_module_level_rebinding_of_a_builtin_is_not_read_as_the_builtin() -> Result<(), String> {
    // Production hands the guard reader the whole new-side file, so a
    // rebinding outside the owner's own `def` is in view. Each form below
    // would make `isinstance(x, str)` mean something other than the builtin,
    // so the guard is unknown and the arm keeps its credit even though no
    // observed input is a `str` (#12171 item 2). The unshadowed arm in
    // `an_isinstance_guard_is_read_for_builtin_classes` is the control that
    // demotes the same call to `WeaklyExposed`.
    let arm = "def label(x):\n    if isinstance(x, str):\n        return 'text'\n";
    let shadows = [
        "isinstance = checker\n\n\n",
        "isinstance: object = checker\n\n\n",
        "def isinstance(value, kind):\n    return True\n\n\n",
        "from checks import isinstance\n\n\n",
        "import checks as isinstance\n\n\n",
        "from checks import *\n\n\n",
        "str = Text\n\n\n",
    ];
    for shadow in shadows {
        let owner = format!("{shadow}{arm}");
        assert_eq!(
            classify(&owner, OLD_NIL, &test_of("label(3)", "None"))?,
            ExposureClass::Exposed,
            "module-level shadow must fail closed: {shadow:?}"
        );
    }
    Ok(())
}

#[test]
fn patching_a_builtin_through_the_builtins_module_is_not_read_as_the_builtin() -> Result<(), String>
{
    // `builtins.isinstance = f` rewrites the builtin without a local binding
    // (#12183), so the guard is unknown and the arm keeps its credit.
    let arm = "def label(x):\n    if isinstance(x, str):\n        return 'text'\n";
    let patches = [
        "import builtins\nbuiltins.isinstance = checker\n\n\n",
        "import builtins\nbuiltins.str = Text\n\n\n",
        "import builtins\nsetattr(builtins, 'isinstance', checker)\n\n\n",
    ];
    for patch in patches {
        let owner = format!("{patch}{arm}");
        assert_eq!(
            classify(&owner, OLD_NIL, &test_of("label(3)", "None"))?,
            ExposureClass::Exposed,
            "builtins patch must fail closed: {patch:?}"
        );
    }
    // Controls: importing `builtins` or calling through it patches nothing.
    for harmless in [
        "import builtins\n\n\n",
        "import builtins\nlimit = builtins.len(CHECKS)\n\n\n",
    ] {
        let owner = format!("{harmless}{arm}");
        assert_eq!(
            classify(&owner, OLD_NIL, &test_of("label(3)", "None"))?,
            ExposureClass::WeaklyExposed,
            "a harmless builtins mention must not suppress the guard: {harmless:?}"
        );
    }
    Ok(())
}

#[test]
fn a_for_body_over_an_empty_literal_never_runs() -> Result<(), String> {
    let body = "def label(items):\n    for item in items:\n        return item + 1\n";
    assert_eq!(
        classify(body, "        return item", &test_of("label([])", "None"))?,
        ExposureClass::WeaklyExposed
    );
    assert_eq!(
        classify(body, "        return item", &test_of("label([2, 3])", "3"))?,
        ExposureClass::Exposed
    );
    // A container the owner touched before the loop may no longer be empty.
    let touched =
        "def label(items):\n    items.append(1)\n    for item in items:\n        return item + 1\n";
    assert_eq!(
        classify(touched, "        return item", &test_of("label([])", "2"))?,
        ExposureClass::Exposed
    );
    // A condition that calls a method on the container may mutate it.
    let popped = "def label(items):\n    if not items.pop():\n        pass\n    for item in items:\n        return item + 1\n";
    assert_eq!(
        classify(popped, "        return item", &test_of("label([])", "2"))?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn unreadable_guard_shapes_keep_the_credit() -> Result<(), String> {
    // `or` hides a second way into the arm.
    let or_guard = "def label(x, force=False):\n    if x is None or force:\n        return 'nil'\n";
    assert_eq!(
        classify(or_guard, OLD_NIL, &test_of("label(3, True)", "'nil'"))?,
        ExposureClass::Exposed
    );
    // A rebound parameter no longer holds the call's literal.
    let rebound = "def label(x):\n    x = None\n    if x is None:\n        return 'nil'\n";
    assert_eq!(
        classify(rebound, OLD_NIL, &test_of("label(3)", "'nil'"))?,
        ExposureClass::Exposed
    );
    // A computed argument is not a literal input.
    let arm = "def label(x):\n    if x is None:\n        return 'nil'\n";
    assert_eq!(
        classify(arm, OLD_NIL, &test_of("label(compute())", "'nil'"))?,
        ExposureClass::Exposed
    );
    // A guard the allow-list does not name.
    let call = "def label(x):\n    if callable(x):\n        return 'fn'\n";
    assert_eq!(
        classify(call, OLD_NIL, &test_of("label(3)", "None"))?,
        ExposureClass::Exposed
    );
    // An `except` arm cannot be evaluated against literal inputs (known limit).
    let handler = "def label(x):\n    try:\n        return int(x)\n    except ValueError:\n        return -1\n";
    assert_eq!(
        classify(handler, "        return 0", &test_of("label('7')", "7"))?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_parenthesized_scalar_is_not_a_one_element_container() -> Result<(), String> {
    // `(None)` is `None`, so the `is None` arm is taken: the credit stays.
    let none_arm = "def label(x):\n    if x is None:\n        return 'nil'\n";
    assert_eq!(
        classify(none_arm, OLD_NIL, &test_of("label((None))", "'nil'"))?,
        ExposureClass::Exposed
    );
    // `(0)` is falsy, so the `not flag` arm is taken: the credit stays.
    let falsy_arm = "def label(flag):\n    if not flag\x3a\n        return 'nil'\n";
    assert_eq!(
        classify(falsy_arm, OLD_NIL, &test_of("label((0))", "'nil'"))?,
        ExposureClass::Exposed
    );
    // `('')` is the empty string, also falsy.
    assert_eq!(
        classify(falsy_arm, OLD_NIL, &test_of("label((''))", "'nil'"))?,
        ExposureClass::Exposed
    );
    // A real one-element tuple `(0,)` is truthy and still demotes the arm.
    assert_eq!(
        classify(falsy_arm, OLD_NIL, &test_of("label((0,))", "'nil'"))?,
        ExposureClass::WeaklyExposed
    );
    Ok(())
}
