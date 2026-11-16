//! Swallowing wrappers (#13723, the negative side of RIPR-SPEC-0238 rule
//! 27): `wrapper(lambda: helper(...))`, where the wrapper provably discards
//! the assert-only helper's `AssertionError`, holds no assertion that can
//! fail the test, so admission completes with nothing assertion-like.

use super::admission::PythonAssertionAdmission;
use super::source_facts::extract_source_facts;
use super::*;

const MUST_EQUAL: &str = "def must_equal(got, want):\n    assert got == want\n";

fn source(prelude: &str, wrapper: &str, helper: &str, call: &str) -> String {
    format!(
        "from pkg.sq import sq\n{prelude}\n\n{wrapper}\n\n\n{helper}\n\n\ndef test_sq():\n    {call}\n"
    )
}

fn admission_of(source: &str) -> PythonAssertionAdmission {
    let facts = extract_source_facts(Path::new("tests/test_sq.py"), source);
    assert_eq!(facts.tests.len(), 1, "fixture must parse to one test");
    facts.tests[0].assertion_admission
}

const CALL: &str = "quiet(lambda: must_equal(sq(3), 9))";

const SWALLOWING: &[(&str, &str)] = &[
    (
        "returns None from an Exception handler",
        "def quiet(fn):\n    try:\n        return fn()\n    except Exception:\n        return None\n",
    ),
    (
        "re-raises ValueError, then returns from an Exception handler",
        "def quiet(fn):\n    try:\n        return fn()\n    except ValueError:\n        raise\n    except Exception:\n        return None\n",
    ),
    (
        "re-raises only an unrelated isinstance match",
        "def quiet(fn):\n    try:\n        return fn()\n    except Exception as exc:\n        if isinstance(exc, ValueError):\n            raise\n",
    ),
    (
        "re-raises but returns from finally",
        "def quiet(fn):\n    try:\n        return fn()\n    except Exception:\n        raise\n    finally:\n        return None\n",
    ),
    (
        "passes in a bare handler",
        "def quiet(fn):\n    \"\"\"Run quietly.\"\"\"\n    try:\n        fn()\n    except:\n        pass\n",
    ),
    (
        "passes in an AssertionError handler",
        "def quiet(fn):\n    try:\n        return fn()\n    except (KeyError, AssertionError):\n        return False\n",
    ),
];

#[test]
fn given_a_swallowing_wrapper_around_an_assert_only_helper_then_no_assertion_can_fail() {
    for (label, wrapper) in SWALLOWING {
        let source = source("", wrapper, MUST_EQUAL, CALL);
        assert_eq!(
            admission_of(&source),
            PythonAssertionAdmission::NoAssertionLike,
            "{label}"
        );
    }
}

#[test]
fn given_a_wrapper_that_lets_the_assertion_escape_then_admission_stays_unresolved() {
    let escaping = [
        (
            "re-raises",
            "def quiet(fn):\n    try:\n        return fn()\n    except Exception:\n        raise\n",
        ),
        (
            "re-raises AssertionError before swallowing Exception",
            "def quiet(fn):\n    try:\n        return fn()\n    except AssertionError:\n        raise\n    except Exception:\n        return None\n",
        ),
        (
            "catches only an unrelated class",
            "def quiet(fn):\n    try:\n        return fn()\n    except ValueError:\n        return None\n",
        ),
        (
            "catches a project class",
            "def quiet(fn):\n    try:\n        return fn()\n    except CheckFailed:\n        return None\n",
        ),
        (
            "re-raises an isinstance match RIPR cannot bound",
            "def quiet(fn):\n    try:\n        return fn()\n    except Exception as exc:\n        if isinstance(exc, AssertionError):\n            raise\n",
        ),
        (
            "runs a statement before the try",
            "def quiet(fn):\n    log()\n    try:\n        return fn()\n    except Exception:\n        return None\n",
        ),
        (
            "calls the callable with an argument",
            "def quiet(fn):\n    try:\n        return fn(1)\n    except Exception:\n        return None\n",
        ),
        (
            "calls something in the handler",
            "def quiet(fn):\n    try:\n        return fn()\n    except Exception:\n        report()\n",
        ),
        (
            "has an else clause",
            "def quiet(fn):\n    try:\n        fn()\n    except Exception:\n        pass\n    else:\n        return None\n",
        ),
        (
            "returns a call from finally",
            "def quiet(fn):\n    try:\n        return fn()\n    finally:\n        return done()\n",
        ),
        (
            "returns a call from the handler",
            "def quiet(fn):\n    try:\n        return fn()\n    except Exception:\n        return report()\n",
        ),
        (
            "calls something in a handler before a swallowing finally",
            "def quiet(fn):\n    try:\n        return fn()\n    except Exception:\n        report()\n    finally:\n        return None\n",
        ),
        (
            "guards the re-raise with an else",
            "def quiet(fn):\n    try:\n        return fn()\n    except Exception as exc:\n        if isinstance(exc, ValueError):\n            raise\n        else:\n            pass\n",
        ),
        (
            "binds Exception as an earlier handler name",
            "def quiet(fn):\n    try:\n        return fn()\n    except ValueError as Exception:\n        raise\n    except Exception:\n        return None\n",
        ),
        (
            "names its parameter Exception",
            "def quiet(Exception):\n    try:\n        return Exception()\n    except Exception:\n        return None\n",
        ),
        (
            "binds the guarded class as the handler name",
            "def quiet(fn):\n    try:\n        return fn()\n    except Exception as ValueError:\n        if isinstance(ValueError, ValueError):\n            raise\n",
        ),
        (
            "binds isinstance as the handler name",
            "def quiet(fn):\n    try:\n        return fn()\n    except Exception as isinstance:\n        if isinstance(isinstance, ValueError):\n            raise\n",
        ),
        (
            "is decorated",
            "@wraps\ndef quiet(fn):\n    try:\n        return fn()\n    except Exception:\n        return None\n",
        ),
    ];
    for (label, wrapper) in escaping {
        let source = source("", wrapper, MUST_EQUAL, CALL);
        assert_eq!(
            admission_of(&source),
            PythonAssertionAdmission::Unresolved,
            "{label}"
        );
    }
}

#[test]
fn given_a_helper_that_can_escape_an_exception_handler_then_admission_stays_unresolved() {
    let quiet = SWALLOWING[0].1;
    let helpers = [
        (
            "calls pytest.fail, a BaseException",
            "def must_equal(got, want):\n    if got != want:\n        pytest.fail('differs')\n",
        ),
        (
            "calls a function in its assert",
            "def must_equal(got, want):\n    assert check(got, want)\n",
        ),
        (
            "reads a global",
            "def must_equal(got, want):\n    assert got == EXPECTED\n",
        ),
        (
            "has a default",
            "def must_equal(got, want=9):\n    assert got == want\n",
        ),
    ];
    for (label, helper) in helpers {
        let source = source("import pytest", quiet, helper, CALL);
        assert_eq!(
            admission_of(&source),
            PythonAssertionAdmission::Unresolved,
            "{label}"
        );
    }
}

#[test]
fn given_a_rebound_or_unproven_name_then_the_swallowing_read_is_refused() {
    let quiet = SWALLOWING[0].1;
    let partial = SWALLOWING[1].1;
    let cases = [
        (
            "rebinds the unrelated class",
            "ValueError = AssertionError",
            partial,
            CALL.to_string(),
        ),
        (
            "rebinds isinstance",
            "def isinstance(value, kind):\n    return False",
            SWALLOWING[2].1,
            CALL.to_string(),
        ),
        (
            "imports builtins",
            "import builtins",
            partial,
            CALL.to_string(),
        ),
        (
            "writes through __builtins__",
            "__builtins__[\"ValueError\"] = AssertionError",
            partial,
            CALL.to_string(),
        ),
        (
            "writes through a function's __globals__",
            "def _x():\n    pass\n_x.__globals__[\"Exception\"] = KeyError",
            quiet,
            CALL.to_string(),
        ),
        (
            "writes through a frame's f_globals",
            "import sys\nsys._getframe().f_globals[\"Exception\"] = KeyError",
            quiet,
            CALL.to_string(),
        ),
        (
            "writes through a frame's f_locals",
            "import sys\nsys._getframe().f_locals[\"Exception\"] = KeyError",
            quiet,
            CALL.to_string(),
        ),
        (
            "writes through a frame's f_builtins",
            "import inspect\ninspect.currentframe().f_builtins[\"Exception\"] = KeyError",
            quiet,
            CALL.to_string(),
        ),
        (
            "captures Exception in a match",
            "match KeyError:\n    case Exception:\n        pass",
            quiet,
            CALL.to_string(),
        ),
        (
            "declares Exception global in a setup hook",
            "def setup_module():\n    global Exception\n    Exception = KeyError",
            quiet,
            CALL.to_string(),
        ),
        (
            "patches Exception through sys.modules",
            "import sys\nsetattr(sys.modules[__name__], \"Exception\", KeyError)",
            quiet,
            CALL.to_string(),
        ),
        (
            "binds the unrelated class with a walrus",
            "if (ValueError := AssertionError):\n    pass",
            partial,
            CALL.to_string(),
        ),
        (
            "rebinds the wrapper",
            "quiet = print",
            quiet,
            CALL.to_string(),
        ),
        (
            "imports the wrapper too",
            "from tests.helpers import quiet",
            quiet,
            CALL.to_string(),
        ),
        (
            "passes the wrapper around",
            "",
            quiet,
            format!("runner = quiet\n    {CALL}"),
        ),
        (
            "star-imports",
            "from tests.helpers import *",
            quiet,
            CALL.to_string(),
        ),
        (
            "runs a lambda with a parameter",
            "",
            quiet,
            "quiet(lambda x=1: must_equal(sq(3), 9))".to_string(),
        ),
        (
            "asserts through an argument of the helper",
            "",
            quiet,
            "quiet(lambda: must_equal(check_sq(3), 9))".to_string(),
        ),
    ];
    for (label, prelude, wrapper, call) in cases {
        let source = source(prelude, wrapper, MUST_EQUAL, &call);
        assert_ne!(
            admission_of(&source),
            PythonAssertionAdmission::NoAssertionLike,
            "{label}"
        );
    }
}

#[test]
fn given_a_swallowed_helper_beside_a_real_assertion_then_the_assertion_is_recognized() {
    let call = format!("{CALL}\n    assert sq(3) == 9");
    let source = source("", SWALLOWING[0].1, MUST_EQUAL, &call);
    assert_eq!(admission_of(&source), PythonAssertionAdmission::Recognized);
}
