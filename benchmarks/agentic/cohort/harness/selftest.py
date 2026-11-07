"""Cheap local/CI proof for the cohort instrument; never launches an agent."""
import argparse
import json
from pathlib import Path
import shutil
import sys
import unittest

# -I intentionally excludes cwd and user-site modules; import only this owner.
ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT))


NATIVE_CONTROLS = frozenset({
    "test_native_workspace.NativeWorkspaceTests.test_custom_manifest_is_isolated_inside_an_enclosing_workspace",
    "test_native_scoring.NativeScoringTests.test_exact_boundary_test_catches_mutant_and_passes_restoration",
    "test_native_scoring.NativeScoringTests.test_smoke_test_survives_the_same_mutant",
    "test_native_scoring.NativeScoringTests.test_zero_tests_is_not_pristine_validation",
    "test_native_scoring.NativeScoringTests.test_noncompiling_mutant_is_unresolved_not_caught",
    "test_native_scoring.NativeScoringTests.test_test_process_exit_is_not_a_caught_assertion",
})


def test_ids(suite):
    for case in suite:
        if isinstance(case, unittest.TestSuite):
            yield from test_ids(case)
        else:
            yield case.id()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--require-native", action="store_true", help="require Git, Cargo and zero skips")
    args = parser.parse_args()
    if not shutil.which("git") or (args.require_native and not shutil.which("cargo")):
        print("cohort selftest needs git and, with --require-native, cargo", file=sys.stderr)
        return 2
    suite = unittest.defaultTestLoader.discover(str(ROOT), pattern="test_*.py")
    missing = NATIVE_CONTROLS - set(test_ids(suite))
    if args.require_native and missing:
        print("cohort selftest missing native controls: " + ", ".join(sorted(missing)), file=sys.stderr)
        return 2
    if suite.countTestCases() == 0:
        print("cohort selftest discovered zero tests", file=sys.stderr)
        return 2
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    summary = dict(tests=result.testsRun, failures=len(result.failures), errors=len(result.errors),
                   skipped=len(result.skipped), native_required=args.require_native)
    print("COHORT_SELFTEST " + json.dumps(summary, sort_keys=True))
    return 0 if result.wasSuccessful() and (not args.require_native or not result.skipped) else 1


if __name__ == "__main__":
    raise SystemExit(main())
