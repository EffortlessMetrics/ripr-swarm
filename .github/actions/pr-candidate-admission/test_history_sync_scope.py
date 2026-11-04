"""Execute the workflow's history-sync scope and result blocks (issue #12437).

These use Bash with controlled job results, not hosted GitHub execution. No
network, candidate code or protected check publication is involved.
"""
from __future__ import annotations

import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
WORKFLOW = ROOT / '.github/workflows/routed-rust.yml'
HEAD = 'a' * 40


def step_block(source: str, name: str) -> str:
    """Extract one literal run block; refuse missing/ambiguous markers."""
    marker = f'      - name: {name}\n'
    if source.count(marker) != 1:
        raise ValueError(f'exactly one step required: {name}')
    rest = source.split(marker, 1)[1]
    rest = rest.split('\n      - ', 1)[0]
    if '        run: |\n' not in rest:
        raise ValueError(f'literal run block missing: {name}')
    body = rest.split('        run: |\n', 1)[1]
    lines = []
    for line in body.splitlines():
        if line and not line.startswith('          '):
            break
        lines.append(line[10:] if line else '')
    if not lines:
        raise ValueError('run block is empty')
    return '\n'.join(lines) + '\n'


def execute(script: str, values: dict[str, str]) -> tuple[int, str, str]:
    if shutil.which('bash') is None:
        raise RuntimeError('Bash is required for executed workflow controls')
    with tempfile.TemporaryDirectory(prefix='ripr-sync-scope-') as directory:
        root = Path(directory)
        output, summary = root / 'outputs', root / 'summary'
        # No inherited shell startup or CI variables can alter the fixture.
        env = {'PATH': os.environ.get('PATH', ''), 'HOME': directory,
               'LANG': 'C.UTF-8', 'GITHUB_OUTPUT': str(output),
               'GITHUB_STEP_SUMMARY': str(summary)}
        env.update(values)
        done = subprocess.run(['bash', '--noprofile', '--norc', '-e', '-c', script],
                              cwd=root, env=env, text=True, capture_output=True,
                              timeout=10, check=False)
        text = summary.read_text() if summary.exists() else ''
        text += output.read_text() if output.exists() else ''
        return done.returncode, text, done.stdout + done.stderr


class HistorySyncScopeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.source = WORKFLOW.read_text(encoding='utf-8')
        cls.scope = step_block(cls.source, 'Select admitted qualification scope')
        cls.result = step_block(cls.source, 'Summarize routed result')

    def scoped(self, **overrides: str) -> tuple[int, str, str]:
        values = dict(DOCS_ONLY='true', ADMISSION_STATUS='admitted',
                      ADMISSION_REASON='admitted', HISTORY_SYNC_STATUS='not_requested',
                      EVENT_NAME='pull_request')
        values.update(overrides)
        return execute(self.scope, values)

    def results(self, **overrides: str) -> tuple[int, str, str]:
        # Give every referenced variable an explicit value. Named successes
        # are supplied below, so absent child evidence never inherits a pass.
        names = re.findall(r'\$\{?([A-Z][A-Z0-9_]*)', self.result)
        values = dict.fromkeys(names, '')
        values.update(ADMISSION_RESULT='success', ADMISSION_STATUS='admitted',
                      ADMISSION_REASON='admitted', ADMISSION_CUT_STATUS='inactive',
                      EVENT_NAME='pull_request', LABEL_NAME='merge-ready',
                      DOCS_DETECT_RESULT='success', DOCS_ONLY='false',
                      DOCS_GATE_RESULT='skipped', ELIGIBILITY_RESULT='success',
                      VSCODE_CHECK_RESULT='success', ROUTE_RESULT='success',
                      TARGET='github', GITHUB_RESULT='success',
                      GITHUB_TESTS_RESULT='success', CX43_RESULT='skipped',
                      CPX42_RESULT='skipped', CX53_RESULT='skipped',
                      GITHUB_PR_EVIDENCE='generated',
                      ADMISSION_HISTORY_SYNC='not_requested', ADMISSION_HEAD=HEAD,
                      HISTORY_SYNC_LIVE_RESULT='skipped',
                      WORKFLOW_REF='refs/pull/42/merge')
        values.update(overrides)
        # The test driver owns these paths, not the workflow fixture data.
        values.pop('GITHUB_STEP_SUMMARY', None)
        values.pop('GITHUB_OUTPUT', None)
        return execute(self.result, values)

    def synced(self, **overrides: str) -> tuple[int, str, str]:
        values = dict(ADMISSION_REASON='history_sync_admitted',
                      ADMISSION_HISTORY_SYNC='validated', HISTORY_SYNC_LIVE_RESULT='success',
                      GITHUB_PR_EVIDENCE=f'not_applicable:history_sync:{HEAD}')
        values.update(overrides)
        return self.results(**values)

    def assert_result(self, observation: tuple[int, str, str], passed: bool,
                      reason: str) -> None:
        status, summary, console = observation
        if passed:
            self.assertEqual(status, 0, console + summary)
            self.assertIn('- status: `pass`', summary)
        else:
            self.assertNotEqual(status, 0, console + summary)
            self.assertIn('- status: `fail`', summary)
        self.assertIn(reason, summary)

    def test_ordinary_docs_remain_docs_only(self) -> None:
        rc, text, console = self.scoped()
        self.assertEqual(rc, 0, console)
        self.assertEqual(text, 'docs_only=true\n')

    def test_ordinary_code_remains_product(self) -> None:
        rc, text, console = self.scoped(DOCS_ONLY='false')
        self.assertEqual(rc, 0, console)
        self.assertEqual(text, 'docs_only=false\n')

    def test_admitted_docs_sync_forces_product(self) -> None:
        rc, text, console = self.scoped(ADMISSION_REASON='history_sync_admitted',
                                       HISTORY_SYNC_STATUS='validated')
        self.assertEqual(rc, 0, console)
        self.assertEqual(text, 'docs_only=false\n')

    def test_scope_refuses_missing_classification(self) -> None:
        rc, _, console = self.scoped(DOCS_ONLY='')
        self.assertNotEqual(rc, 0)
        self.assertIn('docs-only scope is unavailable', console)

    def test_scope_refuses_contradictory_sync_identity(self) -> None:
        for changes in ({'HISTORY_SYNC_STATUS': 'validated'},
                        {'ADMISSION_REASON': 'history_sync_admitted'},
                        {'ADMISSION_REASON': 'history_sync_admitted',
                         'HISTORY_SYNC_STATUS': 'validated', 'ADMISSION_STATUS': 'refused'},
                        {'ADMISSION_REASON': 'history_sync_admitted',
                         'HISTORY_SYNC_STATUS': 'validated', 'EVENT_NAME': 'merge_group'}):
            with self.subTest(changes=changes):
                rc, _, console = self.scoped(**changes)
                self.assertNotEqual(rc, 0)
                self.assertIn('history-sync admission is contradictory', console)

    def test_rejected_sync_does_not_gain_exception(self) -> None:
        rc, text, console = self.scoped(HISTORY_SYNC_STATUS='rejected')
        self.assertEqual(rc, 0, console)
        self.assertEqual(text, 'docs_only=true\n')

    def test_ordinary_docs_result_unchanged(self) -> None:
        self.assert_result(self.results(DOCS_ONLY='true', DOCS_GATE_RESULT='success',
                                        GITHUB_RESULT='skipped', GITHUB_TESTS_RESULT='skipped'),
                           True, 'docs-only surface; heavy lanes skipped; docs-gate passed')

    def test_valid_sync_requires_executed_product_and_exact_evidence(self) -> None:
        self.assert_result(self.synced(), True, 'selected implementation job succeeded')

    def test_docs_only_gate_cannot_qualify_history_sync(self) -> None:
        self.assert_result(self.synced(DOCS_ONLY='true', DOCS_GATE_RESULT='success',
                                       GITHUB_RESULT='skipped', GITHUB_TESTS_RESULT='skipped'),
                           False, 'full product scope')

    def test_no_implementation_is_not_a_sync_success(self) -> None:
        self.assert_result(self.synced(GITHUB_RESULT='skipped', GITHUB_TESTS_RESULT='skipped'),
                           False, 'requires a succeeded implementation lane')

    def test_missing_typed_evidence_refuses(self) -> None:
        self.assert_result(self.synced(GITHUB_PR_EVIDENCE=''), False,
                           'requires github to record PR evidence')

    def test_wrong_head_evidence_refuses(self) -> None:
        self.assert_result(self.synced(GITHUB_PR_EVIDENCE='not_applicable:history_sync:' + 'b' * 40),
                           False, 'requires github to record PR evidence')

    def test_malformed_head_refuses(self) -> None:
        self.assert_result(self.synced(ADMISSION_HEAD='short'), False,
                           'consistent exact-head admission')

    def test_nonvalidated_typed_skip_refuses(self) -> None:
        self.assert_result(self.results(GITHUB_PR_EVIDENCE=f'not_applicable:history_sync:{HEAD}'),
                           False, 'without a history-sync admission')

    def test_contradictory_sync_status_refuses(self) -> None:
        self.assert_result(self.synced(ADMISSION_HISTORY_SYNC='rejected'), False,
                           'consistent exact-head admission')

    def test_queue_cannot_borrow_sync_exception(self) -> None:
        self.assert_result(self.synced(EVENT_NAME='merge_group'), False,
                           'consistent exact-head admission')

    def test_late_sync_continuity_remains_mandatory(self) -> None:
        for result in ('failure', 'skipped', 'cancelled', ''):
            with self.subTest(result=result):
                self.assert_result(self.synced(HISTORY_SYNC_LIVE_RESULT=result), False,
                                   'authorization or composition continuity is not established')

    def test_hosted_shards_remain_mandatory(self) -> None:
        for result in ('failure', 'skipped', 'cancelled', ''):
            with self.subTest(result=result):
                self.assert_result(self.synced(GITHUB_TESTS_RESULT=result), False,
                                   'GitHub-hosted sharded tests result')

    def test_each_succeeded_implementation_requires_exact_record(self) -> None:
        self.assert_result(self.synced(TARGET='cx43', CX43_RESULT='success',
                                       CX43_PR_EVIDENCE='generated'), False,
                           'requires cx43 to record PR evidence')

    def test_ordinary_hosted_product_still_passes(self) -> None:
        self.assert_result(self.results(), True, 'selected implementation job succeeded')

    def test_live_eligibility_failure_still_refuses(self) -> None:
        self.assert_result(self.synced(ELIGIBILITY_RESULT='failure'), False,
                           'live eligibility no longer established')

    def test_routing_and_result_consume_post_admission_scope(self) -> None:
        admission = self.source.split('  admission:\n', 1)[1].split('  rust-cx43:\n', 1)[0]
        self.assertIn('docs_only: ${{ steps.scope.outputs.docs_only }}', admission)
        self.assertIn("docs-only: ${{ needs.detect-docs-only.outputs.docs_only || 'false' }}", admission)
        downstream = self.source.split('  rust-cx43:\n', 1)[1]
        self.assertNotIn('needs.detect-docs-only.outputs.docs_only', downstream)
        for job in ('rust-cpx42', 'rust-cx53', 'rust-github', 'docs-gate'):
            block = self.source.split(f'  {job}:\n', 1)[1].split('    steps:', 1)[0]
            self.assertIn('needs.admission.outputs.docs_only', block)
        self.assertIn('DOCS_ONLY: ${{ needs.admission.outputs.docs_only }}', downstream)


if __name__ == '__main__':
    unittest.main()
