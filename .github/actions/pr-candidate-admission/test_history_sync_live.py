"""Controlled readback tests for the history-sync continuity witness.

The canonical ancestry validator is injected here; these tests prove continuity,
not the GitHub transport, canonical validator, or atomic protection enforcement.
"""
from __future__ import annotations

from copy import deepcopy
import unittest

from history_sync_live import Refused, fingerprint, snapshot

REPO = 'EffortlessMetrics/ripr-swarm'
ROOT = 'repos/' + REPO
HEAD, BASE, GROUP, TREE, SOURCE_SHA = (char * 40 for char in 'abcde')
SOURCE = 'EffortlessMetrics/ripr@' + SOURCE_SHA
PR = ROOT + '/pulls/42'
REF = ROOT + '/git/ref/heads/main'
EVENTS = ROOT + '/issues/42/events?per_page=100'
ROLE = ROOT + '/collaborators/owner/permission'
COMMIT = ROOT + '/git/commits/' + GROUP


class Fixture:
    def __init__(self):
        self.rows = {
            PR: {'number': 42, 'state': 'open', 'draft': False, 'body': 'declared inputs',
                 'head': {'sha': HEAD, 'repo': {'full_name': REPO}},
                 'base': {'sha': BASE, 'ref': 'main', 'repo': {'full_name': REPO}},
                 'merge_commit_sha': GROUP,
                 'labels': [{'name': 'history-sync'}, {'name': 'merge-ready'}]},
            REF: {'object': {'sha': BASE}},
            EVENTS: [{'id': 11, 'created_at': '2026-11-03T01:00:00Z', 'event': 'labeled',
                      'label': {'name': 'history-sync'}, 'actor': {'login': 'owner'}}],
            ROLE: {'role_name': 'maintain'},
            COMMIT: {'sha': GROUP, 'parents': [{'sha': BASE}, {'sha': HEAD}],
                     'tree': {'sha': TREE}},
        }
        self.verdict = {'status': 'validated', 'source': 'EffortlessMetrics/ripr',
                        'source_sha': SOURCE_SHA, 'destination_base': BASE}
        self.reads = {}
        self.after = {}
        self.record_events = True

    def api(self, path, **kwargs):
        self.reads[path] = self.reads.get(path, 0) + 1
        value = self.after.get(path) if self.reads[path] > 1 and path in self.after else self.rows.get(path)
        return deepcopy(value)

    def validate(self, current, repository, event, base, api):
        assert (repository, event, base) == (REPO, 'pull_request', BASE)
        if self.record_events:
            api(EVENTS, paginate=True)
        api(ROLE)
        return deepcopy(self.verdict)

    def observe(self, **kwargs):
        args = dict(repository=REPO, number=42, head=HEAD, base=BASE,
                    integration=GROUP, source=SOURCE, destination_base=BASE)
        args.update(kwargs)
        return snapshot(self.api, self.validate, **args)


class ContinuityTests(unittest.TestCase):
    def setUp(self):
        self.f = Fixture()

    def refuses(self, reason):
        with self.assertRaisesRegex(Refused, reason):
            self.f.observe()

    def test_complete_subject_has_stable_fingerprint(self):
        first = self.f.observe()
        self.assertEqual(first['head'], HEAD)
        self.assertEqual(first['integration'], GROUP)
        self.assertEqual(first['tree'], TREE)
        self.assertEqual(first['label_event'], 11)
        self.assertEqual(first['role'], 'maintain')
        self.assertEqual(fingerprint(first), fingerprint(self.f.observe()))

    def test_moved_head_refuses(self):
        self.f.rows[PR]['head']['sha'] = 'f' * 40
        self.refuses('live PR no longer')

    def test_draft_refuses(self):
        self.f.rows[PR]['draft'] = True
        self.refuses('live PR no longer')

    def test_fork_refuses(self):
        self.f.rows[PR]['head']['repo']['full_name'] = 'elsewhere/fork'
        self.refuses('live PR no longer')

    def test_changed_pr_base_refuses(self):
        self.f.rows[PR]['base']['sha'] = 'f' * 40
        self.refuses('live PR no longer')

    def test_recomposed_live_merge_refuses(self):
        self.f.rows[PR]['merge_commit_sha'] = 'f' * 40
        self.refuses('live PR no longer')

    def test_missing_either_label_refuses(self):
        for removed in ('history-sync', 'merge-ready'):
            self.f = Fixture()
            self.f.rows[PR]['labels'] = [row for row in self.f.rows[PR]['labels'] if row['name'] != removed]
            with self.subTest(removed=removed):
                self.refuses('live PR no longer')

    def test_moved_main_refuses(self):
        self.f.rows[REF]['object']['sha'] = 'f' * 40
        self.refuses('destination main moved')

    def test_canonical_rejection_refuses(self):
        self.f.verdict['status'] = 'rejected'
        self.refuses('canonical history-sync admission no longer')

    def test_changed_declared_source_refuses(self):
        self.f.verdict['source_sha'] = 'f' * 40
        self.refuses('source declaration changed')

    def test_changed_declared_destination_refuses(self):
        self.f.verdict['destination_base'] = 'f' * 40
        self.refuses('source declaration changed')

    def test_unobserved_event_refuses(self):
        self.f.record_events = False
        self.refuses('authorized label event was not observed')

    def test_removed_label_event_refuses_even_if_label_list_is_stale(self):
        self.f.rows[EVENTS].append({'id': 12, 'created_at': '2026-11-03T02:00:00Z',
                                  'event': 'unlabeled', 'label': {'name': 'history-sync'},
                                  'actor': {'login': 'owner'}})
        self.refuses('authorization was withdrawn')

    def test_reapplied_authorized_label_changes_witness(self):
        initial = fingerprint(self.f.observe())
        self.f.rows[EVENTS][0]['id'] = 12
        self.assertNotEqual(initial, fingerprint(self.f.observe()))

    def test_changed_actor_role_is_not_accepted(self):
        self.f.rows[ROLE]['role_name'] = 'write'
        self.refuses('authorizing actor no longer')

    def test_same_head_changed_authorization_is_visible(self):
        initial = fingerprint(self.f.observe())
        self.f.rows[ROLE]['role_name'] = 'admin'
        self.assertNotEqual(initial, fingerprint(self.f.observe()))

    def test_missing_event_identity_refuses(self):
        self.f.rows[EVENTS][0].pop('id')
        self.refuses('authorized label event identity is invalid')

    def test_wrong_commit_resource_refuses(self):
        self.f.rows[COMMIT]['sha'] = 'f' * 40
        self.refuses('integration parents differ')

    def test_reversed_parents_refuse(self):
        self.f.rows[COMMIT]['parents'].reverse()
        self.refuses('integration parents differ')

    def test_extra_parent_refuses(self):
        self.f.rows[COMMIT]['parents'].append({'sha': 'f' * 40})
        self.refuses('integration parents differ')

    def test_missing_tree_refuses(self):
        self.f.rows[COMMIT]['tree'] = {}
        self.refuses('integration tree is unavailable')

    def test_changed_body_during_readback_refuses(self):
        self.f.after[PR] = deepcopy(self.f.rows[PR])
        self.f.after[PR]['body'] = 'other controls'
        self.refuses('subject moved while reading')

    def test_changed_main_during_readback_refuses(self):
        self.f.after[REF] = {'object': {'sha': 'f' * 40}}
        self.refuses('subject moved while reading')

    def test_unreadable_live_pr_refuses(self):
        self.f.rows[PR] = None
        self.refuses('live PR no longer')

    def test_boolean_pr_identity_is_invalid(self):
        with self.assertRaisesRegex(Refused, 'invalid repository or PR identity'):
            self.f.observe(number=True)

    def test_short_commit_identity_is_invalid(self):
        with self.assertRaisesRegex(Refused, 'invalid exact commit identity'):
            self.f.observe(integration='abc')


if __name__ == '__main__':
    unittest.main()
