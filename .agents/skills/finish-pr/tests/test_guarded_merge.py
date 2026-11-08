import copy
import importlib.util
import json
from types import SimpleNamespace
from unittest.mock import patch
from pathlib import Path
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts' / 'guarded_merge.py'
SPEC = importlib.util.spec_from_file_location('guarded_merge', SCRIPT)
g = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(g)
HEAD = 'a' * 40


def page(key, *items, total=None):
    return [{'total_count': len(items) if total is None else total, key: list(items)}]


def run(**kw):
    r = dict(id=10, head_sha=HEAD, event='pull_request', run_attempt=1,
             created_at='2026-01-01T00:00:00Z', pull_requests=[{'number': 42}], display_title=f'Merge-ready request {HEAD}', status='completed', conclusion='success', check_suite_id=20)
    r.update(kw)
    return r


def job(**kw):
    j = dict(id=30, name=g.RESULT, run_id=10, run_attempt=1, head_sha=HEAD, status='completed', conclusion='success')
    j.update(kw)
    return j


def check(**kw):
    c = dict(id=30, name=g.RESULT, head_sha=HEAD, check_suite={'id': 20}, app={'id': g.RESULT_APP},
             status='completed', conclusion='success', details_url='https://github.com/o/r/actions/runs/10/job/30')
    c.update(kw)
    return c


class API:
    def __init__(self):
        self.number = 42
        self.repo = "owner/repo"
        self.r = page('workflow_runs', run())
        self.j = page('jobs', job())
        self.c = page('check_runs', check())
        self.prs = [dict(state='open', draft=False, head={'sha': HEAD}, mergeable=True, mergeable_state='clean') for _ in range(2)]
        self.mutations = []
    def pr(self): return self.prs.pop(0)
    def runs(self, head): return copy.deepcopy(self.r)
    def jobs(self, run_id, attempt): return copy.deepcopy(self.j)
    def suite_checks(self, suite): return copy.deepcopy(self.c)
    def merge(self, head):
        self.mutations.append(head)
        return {'merged': True}


class NativeAPI(API):
    def __init__(self):
        super().__init__()
        self.repo = g.AUDITED_REPO
        self.r = page('workflow_runs', run(pull_requests=[]))
        self.merge_sha = 'd' * 40
        self.base_sha = 'e' * 40
        self.detail = dict(id=10, run_attempt=1, head_sha=HEAD, event='pull_request',
                           status='completed', check_suite_id=20, workflow_id=g.AUDITED_WORKFLOW_ID,
                           path='.github/workflows/routed-rust.yml', repository={'id': g.AUDITED_REPO_ID},
                           referenced_workflows=[dict(path=f'{g.AUDITED_REPO}/.github/workflows/{name}@{self.merge_sha}',
                                                      sha=self.merge_sha, ref='refs/pull/42/merge')
                                                 for name in ('rust-gates.yml', 'rust-test-shards.yml')])
        self.commit_data = dict(sha=self.merge_sha, parents=[{'sha': self.base_sha}, {'sha': HEAD}])
        self.blobs = dict(zip(g.WORKFLOW_NAMES, next(iter(g.AUDITED_WORKFLOW_TUPLES))))
    def attempt(self, run, attempt): return copy.deepcopy(self.detail)
    def commit(self, sha): return copy.deepcopy(self.commit_data)
    def workflow_file(self, name, ref): return {'sha': self.blobs[name]}


class Tests(unittest.TestCase):
    def refuse(self, api):
        with self.assertRaises(g.Refusal): g.guarded_merge(api, HEAD)
        self.assertEqual(api.mutations, [])

    def test_success_and_unstable_do_not_add_advisory_gate(self):
        for state in ('clean', 'unstable'):
            a = NativeAPI()
            for pr in a.prs: pr['mergeable_state'] = state
            g.guarded_merge(a, HEAD)
            self.assertEqual(a.mutations, [HEAD])

    def test_unrelated_workflow_failure_is_not_an_extra_gate(self):
        a=NativeAPI(); a.r=page('workflow_runs', run(conclusion='failure'))
        g.guarded_merge(a, HEAD)
        self.assertEqual(a.mutations, [HEAD])

    def test_latest_request_overrides_execution_start_order(self):
        a = NativeAPI()
        a.r = page('workflow_runs', run(created_at='2026-01-01T00:00:00Z', run_started_at='2026-01-03T00:00:00Z'),
                   run(id=11, created_at='2026-01-02T00:00:00Z', run_started_at='2026-01-02T00:00:00Z', conclusion='failure'))
        self.refuse(a)

    def test_metadata_title_cannot_hide_newer_or_rerun_request(self):
        # A changed caller may make an actual newer request look like metadata.
        for status, conclusion in (('queued', None), ('completed', 'failure')):
            a=NativeAPI(); a.r=page('workflow_runs', run(), run(id=11, created_at='2026-01-02T00:00:00Z', display_title='Label metadata 11', status=status, conclusion=conclusion))
            self.refuse(a)
        a=NativeAPI(); a.r=page('workflow_runs', run(), run(id=11, created_at='2025-12-31T00:00:00Z', display_title='Label metadata 11', run_attempt=2))
        self.refuse(a)
        a=NativeAPI(); a.r=page('workflow_runs', run(), run(id=11, created_at='2025-12-31T00:00:00Z', display_title='Label metadata 11'))
        g.guarded_merge(a, HEAD); self.assertEqual(a.mutations, [HEAD])
        a=NativeAPI(); a.r=page('workflow_runs', run(), run(id=11, display_title='unknown'))
        self.refuse(a)

    def test_latest_queued_or_missing_result_refuses(self):
        a = NativeAPI(); a.r = page('workflow_runs', run(), run(id=11, created_at='2026-01-02T00:00:00Z', status='queued', conclusion=None))
        self.refuse(a)
        a = NativeAPI(); a.j = page('jobs'); self.refuse(a)

    def test_audited_native_binding_accepts_exact_pr(self):
        a=NativeAPI(); g.guarded_merge(a, HEAD); self.assertEqual(a.mutations, [HEAD])

    def test_native_binding_rejects_wrong_or_missing_evidence(self):
        for field, value in (('id',11), ('run_attempt',2), ('head_sha','b'*40), ('event','pull_request_target'),
                             ('workflow_id',99), ('repository',None), ('path','other'), ('referenced_workflows',[])):
            with self.subTest(field=field):
                a=NativeAPI(); a.detail[field]=value; self.refuse(a)
        for mutation in ('wrong_pr', 'different_sha', 'wrong_path', 'malformed_ref', 'wrong_parent', 'changed_caller', 'changed_local'):
            with self.subTest(mutation=mutation):
                a=NativeAPI()
                if mutation == 'wrong_pr': a.detail['referenced_workflows'][0]['ref']='refs/pull/43/merge'
                if mutation == 'different_sha': a.detail['referenced_workflows'][0]['sha']='f'*40
                if mutation == 'wrong_path': a.detail['referenced_workflows'][0]['path']='elsewhere'
                if mutation == 'malformed_ref': a.detail['referenced_workflows'][0]['path']=None
                if mutation == 'wrong_parent': a.commit_data['parents'].reverse()
                if mutation == 'changed_caller': a.blobs['routed-rust.yml']='f'*40
                if mutation == 'changed_local': a.blobs['rust-gates.yml']='f'*40
                self.refuse(a)

    def test_nonempty_association_does_not_bypass_workflow_audit(self):
        a=NativeAPI(); a.r=page('workflow_runs', run(pull_requests=[{'number': 42}]))
        g.guarded_merge(a, HEAD); self.assertEqual(a.mutations, [HEAD])
        for name in ('routed-rust.yml', 'rust-gates.yml', 'rust-test-shards.yml'):
            with self.subTest(name=name):
                a=NativeAPI(); a.r=page('workflow_runs', run(pull_requests=[{'number': 42}]))
                a.blobs[name]='f'*40
                self.refuse(a)

    def test_mixed_audited_tuples_are_not_individually_whitelisted(self):
        first = next(iter(g.AUDITED_WORKFLOW_TUPLES))
        second = ('1'*40, '2'*40, '3'*40)
        with patch.object(g, 'AUDITED_WORKFLOW_TUPLES', frozenset({first, second})):
            a=NativeAPI(); a.blobs=dict(zip(g.WORKFLOW_NAMES, (first[0], second[1], first[2])))
            self.refuse(a)
            a=NativeAPI(); a.blobs=dict(zip(g.WORKFLOW_NAMES, second))
            g.guarded_merge(a, HEAD); self.assertEqual(a.mutations, [HEAD])

    def test_observed_nonrequest_title_alone_does_not_qualify(self):
        # Run/PR/head/title are observed; unconsulted fake fields are modeled.
        a=NativeAPI(); a.number=11994
        head='0231fd675dca97f036895f051e343f2369dcc917'
        for pr in a.prs: pr['head']['sha']=head
        a.r=page('workflow_runs', run(id=41257225533, head_sha=head,
                 display_title='Label metadata 41257225533', pull_requests=[],
                 check_suite_id=111827598300))
        with self.assertRaisesRegex(g.Refusal, 'qualification request is missing'):
            g.guarded_merge(a, head)
        self.assertEqual(a.mutations, [])

    def test_skipped_reusable_jobs_and_absent_result_do_not_qualify(self):
        # Real run, PR, merge, base, head and suite identities. The title is
        # intentionally changed to a request so the no-Result check is reached;
        # modeled skipped jobs use synthetic IDs.
        a=NativeAPI(); a.number=11994
        a.blobs=dict(zip(g.WORKFLOW_NAMES, (
            '703237707f6270284f5aedaa1af9b174f950f697',
            'f6a3d011c7014bcc7fe53c176ac49541e06ec5a5',
            'a7b828c0f26d08c34c3a9b3b0e4f02c9427daac0')))
        head='0231fd675dca97f036895f051e343f2369dcc917'
        a.merge_sha='40826c149cac126d47237016fbe3ce74fde813dd'
        a.base_sha='c5fa19fd9051f00a9195b37af9c78e5028b6982f'
        a.commit_data=dict(sha=a.merge_sha, parents=[{'sha':a.base_sha}, {'sha':head}])
        for pr in a.prs: pr['head']['sha']=head
        a.r=page('workflow_runs', run(id=41257225533, head_sha=head,
                 display_title=f'Merge-ready request {head}', pull_requests=[], check_suite_id=111827598300))
        a.detail.update(id=41257225533, head_sha=head, check_suite_id=111827598300)
        for ref in a.detail['referenced_workflows']:
            name=ref['path'].split('/')[-1].split('@')[0]
            ref.update(ref='refs/pull/11994/merge', sha=a.merge_sha,
                       path=f'{g.AUDITED_REPO}/.github/workflows/{name}@{a.merge_sha}')
        a.j=page('jobs', job(id=31, name='Rust Gates', head_sha=head, run_id=41257225533, conclusion='skipped'),
                 job(id=32, name='Rust Test Shards', head_sha=head, run_id=41257225533, conclusion='skipped'))
        a.c=page('check_runs')
        with self.assertRaisesRegex(g.Refusal, 'Result job is missing or ambiguous'):
            g.guarded_merge(a, head)
        self.assertEqual(a.mutations, [])

    def test_nested_association_head_cannot_override_immutable_identity(self):
        a=NativeAPI(); a.r=page('workflow_runs', run(pull_requests=[{'number':42, 'head':{'sha':'b'*40}}]))
        g.guarded_merge(a, HEAD); self.assertEqual(a.mutations, [HEAD])
        a=NativeAPI(); a.r=page('workflow_runs', run(pull_requests=[{'number':42, 'head':{'sha':HEAD}}]))
        a.commit_data['parents'][1]['sha']='b'*40
        self.refuse(a)
        a=NativeAPI(); a.r=page('workflow_runs', run(pull_requests=[{'number':42, 'head':{'sha':HEAD}}]))
        a.j=page('jobs', job(head_sha='b'*40)); self.refuse(a)
        a=NativeAPI(); a.prs[1]['head']['sha']='b'*40
        self.refuse(a)

    def test_same_head_wrong_or_unbound_pr_refuses(self):
        for prs in ([{'number': 43}], [{'number': 42}, {'number': 43}], None):
            with self.subTest(prs=prs):
                a=NativeAPI(); a.r=page('workflow_runs', run(pull_requests=prs)); self.refuse(a)

    def test_rerun_order_ambiguous_and_inventory_capped(self):
        a = NativeAPI(); a.r = page('workflow_runs', run(run_attempt=2), run(id=11, created_at='2026-01-02T00:00:00Z')); self.refuse(a)
        a = NativeAPI(); a.r = page('workflow_runs', run(), total=1000); self.refuse(a)
        a = NativeAPI(); a.c = page('check_runs', check(), total=2); self.refuse(a)

    def test_missing_wrong_or_non_green_evidence(self):
        for change in ({'head_sha':'b'*40}, {'app':{'id':42}}, {'app':None}, {'check_suite':None}, {'conclusion':'neutral'}, {'conclusion':'skipped'}, {'conclusion':'failure'}, {'status':'in_progress'}, {'details_url':'https://example.invalid/other'}, {'check_suite':{'id':21}}):
            with self.subTest(change=change):
                a=NativeAPI(); a.c=page('check_runs', check(**change)); self.refuse(a)
        a=NativeAPI(); a.j=page('jobs', job(), job(id=31)); self.refuse(a)
        a=NativeAPI(); a.r=page('workflow_runs', run(), run(id=11)); self.refuse(a)

    def test_wrong_job_run_attempt_or_head_refuses(self):
        for change in ({'run_id': 11}, {'run_attempt': 2}, {'head_sha': 'b'*40}):
            with self.subTest(change=change):
                a=NativeAPI(); a.j=page('jobs', job(**change)); self.refuse(a)

    def test_observed_head_race_and_blocked_state(self):
        a=NativeAPI(); a.prs[1]['head']['sha']='b'*40; self.refuse(a)
        a=NativeAPI(); a.prs[0]['mergeable_state']='blocked'; self.refuse(a)

    def test_changed_request_between_reads(self):
        class Change(NativeAPI):
            def runs(self, head):
                result=copy.deepcopy(self.r)
                self.r=page('workflow_runs', run(), run(id=11, created_at='2026-01-02T00:00:00Z', status='queued', conclusion=None))
                return result
        self.refuse(Change())

    def test_gh_transport_exactly_one_expected_sha_put(self):
        sample = NativeAPI()
        evidence = [sample.r, sample.detail, sample.commit_data] + [
            {'sha': blob} for _ in range(2) for blob in sample.blobs.values()
        ] + [sample.j, sample.c]
        responses = iter([sample.prs[0], *evidence, sample.prs[1], *evidence, {'merged': True}])
        calls = []
        def invoke(args, **kwargs):
            calls.append(args)
            return SimpleNamespace(returncode=0, stdout=json.dumps(next(responses)), stderr='')
        with patch.object(g.subprocess, 'run', side_effect=invoke):
            g.guarded_merge(g.Gh(g.AUDITED_REPO, 42), HEAD)
        puts = [args for args in calls if 'PUT' in args]
        self.assertEqual(len(puts), 1)
        self.assertEqual(puts[0], ['gh', 'api', '--method', 'PUT', f'repos/{g.AUDITED_REPO}/pulls/42/merge',
                                   '-f', 'merge_method=squash', '-f', f'sha={HEAD}'])
        pages = [args for args in calls if '--paginate' in args]
        self.assertEqual(len(pages), 6)
        for args in pages: self.assertIn('--slurp', args)

    def test_gh_transport_server_race_rejects_without_successful_mutation(self):
        sample = NativeAPI()
        evidence = [sample.r, sample.detail, sample.commit_data] + [
            {'sha': blob} for _ in range(2) for blob in sample.blobs.values()
        ] + [sample.j, sample.c]
        responses = iter([sample.prs[0], *evidence, sample.prs[1], *evidence])
        calls = []
        def invoke(args, **kwargs):
            calls.append(args)
            if 'PUT' in args:
                return SimpleNamespace(returncode=1, stdout='', stderr='head changed')
            return SimpleNamespace(returncode=0, stdout=json.dumps(next(responses)), stderr='')
        with patch.object(g.subprocess, 'run', side_effect=invoke):
            with self.assertRaisesRegex(g.Refusal, 'read PR state before retry'):
                g.guarded_merge(g.Gh(g.AUDITED_REPO, 42), HEAD)
        self.assertEqual(sum('PUT' in args for args in calls), 1)
        self.assertIn(f'sha={HEAD}', calls[-1])

    def test_server_rejected_race_requires_readback(self):
        class Race(NativeAPI):
            def merge(self, head): self.mutations.append(head); return {'merged':False}
        a=Race()
        with self.assertRaisesRegex(g.Refusal, 'read PR state before retry'): g.guarded_merge(a, HEAD)
        self.assertEqual(a.mutations,[HEAD])

if __name__ == '__main__': unittest.main()
