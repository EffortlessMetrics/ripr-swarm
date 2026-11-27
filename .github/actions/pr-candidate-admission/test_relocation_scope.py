"""Execute the Result block's verified-relocation evidence rules (#15217).

Same fixture driver as the history-sync controls: Bash over the literal run
block with controlled job results; no network or GitHub execution.
"""
from __future__ import annotations

import unittest

from test_history_sync_scope import HEAD, HistorySyncScopeTests

RECORD = f'not_applicable:mechanical_relocation:{HEAD}'


class RelocationScopeTests(HistorySyncScopeTests):
    def relocated(self, **overrides: str) -> tuple[int, str, str]:
        values = dict(ADMISSION_REASON='mechanical_relocation_admitted', GITHUB_PR_EVIDENCE=RECORD)
        values.update(overrides)
        return self.results(**values)

    def test_verified_relocation_record_passes(self) -> None:
        self.assert_result(self.relocated(), True, f'github pr evidence: `{RECORD}`')

    def test_relocation_in_its_singleton_queue_group_passes(self) -> None:
        self.assert_result(self.relocated(EVENT_NAME='merge_group'), True, f'github pr evidence: `{RECORD}`')

    def test_relocation_requires_its_record(self) -> None:
        for evidence in ('', 'produced', f'not_applicable:history_sync:{HEAD}',
                         'not_applicable:mechanical_relocation:' + 'b' * 40):
            with self.subTest(evidence=evidence):
                self.assert_result(self.relocated(GITHUB_PR_EVIDENCE=evidence), False,
                                   'requires github to record PR evidence')

    def test_relocation_record_without_the_admission_refuses(self) -> None:
        self.assert_result(self.results(GITHUB_PR_EVIDENCE=RECORD), False,
                           'without a history-sync admission or a verified relocation')

    def test_relocation_requires_exact_head_full_scope_admission(self) -> None:
        for overrides in (dict(ADMISSION_HEAD='short'), dict(DOCS_ONLY='true'),
                          dict(ADMISSION_STATUS='refused')):
            with self.subTest(**overrides):
                status, summary, console = self.relocated(**overrides)
                self.assertNotEqual(status, 0, console + summary)
                self.assertIn('- status: `fail`', summary)

    def test_relocation_cannot_ride_a_history_sync(self) -> None:
        self.assert_result(self.relocated(ADMISSION_HISTORY_SYNC='validated'), False,
                           'consistent exact-head admission')

    def test_relocation_requires_a_succeeded_implementation_lane(self) -> None:
        self.assert_result(self.relocated(GITHUB_RESULT='skipped'), False, 'fail')


def load_tests(loader: unittest.TestLoader, tests: unittest.TestSuite, pattern: str | None) -> unittest.TestSuite:
    """Run only the relocation controls; test_history_sync_scope runs its own."""
    names = sorted(name for name in vars(RelocationScopeTests) if name.startswith('test_'))
    return unittest.TestSuite(RelocationScopeTests(name) for name in names)


if __name__ == '__main__':
    unittest.main()
