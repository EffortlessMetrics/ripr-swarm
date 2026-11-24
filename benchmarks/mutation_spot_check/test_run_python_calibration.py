#!/usr/bin/env python3
"""Unit tests for run_python_calibration helpers (#12901). Stdlib only."""
import sys, unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import run_python_calibration as cal


def finding(owner, classification="exposed"):
    return {"classification": classification,
            "probe": {"file": "mod.py", "line": 3, "owner": owner}}


CHECK = {"analysis_outcome": {"analysis_complete": True}}


class VerdictForTests(unittest.TestCase):
    def test_multiple_ownerless_findings_are_ambiguous(self):
        check = dict(CHECK, findings=[finding(""), finding("")])
        self.assertEqual(cal.verdict_for(check, "mod.py", 3),
                         ("", "", "ambiguous_verdict"))

    def test_single_finding_is_canonical(self):
        check = dict(CHECK, findings=[finding("mod.fn")])
        self.assertEqual(cal.verdict_for(check, "mod.py", 3),
                         ("exposed", "strongly_gripped", "canonical_precise"))


class RuntimeOutcomeTests(unittest.TestCase):
    def test_exit_codes(self):
        self.assertEqual(cal.runtime_outcome(0), "missed")
        self.assertEqual(cal.runtime_outcome(1), "caught")
        for code in (2, 3, 4, 5):
            self.assertEqual(cal.runtime_outcome(code), "unscoreable")


class RunTests(unittest.TestCase):
    def test_stdout_is_not_polluted_by_stderr(self):
        code, out, err = cal.run(
            [sys.executable, "-c",
             "import sys; print('{\"ok\": true}'); print('diag', file=sys.stderr)"],
            Path.cwd())
        self.assertEqual(code, 0)
        self.assertEqual(out.strip(), '{"ok": true}')
        self.assertEqual(err.strip(), "diag")

    def test_bare_names_are_not_resolved(self):
        self.assertEqual(cal.resolve_path_arg("python"), "python")

    def test_path_valued_args_are_resolved_absolute(self):
        resolved = Path(cal.resolve_path_arg("target/debug/ripr"))
        self.assertTrue(resolved.is_absolute())


if __name__ == "__main__":
    unittest.main()
