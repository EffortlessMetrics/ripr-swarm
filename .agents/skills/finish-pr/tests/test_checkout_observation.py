import copy
import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import textwrap
import unittest

ROOT = Path(__file__).resolve().parents[4]
SCRIPT = Path(__file__).resolve().parents[1] / 'scripts' / 'checkout_observation.py'
SPEC = importlib.util.spec_from_file_location('checkout_observation', SCRIPT)
m = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(m)
HEAD, BASE, TREE, COMMIT, CALLER, LEAF, GENERATION = (c * 40 for c in 'abcdef0')
PARAMS = dict(pr=42, run_id=10, attempt=1, job_id=30, head=HEAD, base=BASE,
              tree=TREE, caller_blob=CALLER, leaf_blob=LEAF)


def step(name, number, **kw):
    return dict(name=name, number=number, status=kw.get('status', 'completed'),
                conclusion=kw.get('conclusion', 'success'))


class API:
    def __init__(self):
        self.job = dict(id=30, run_id=10, run_attempt=1, head_sha=HEAD,
                        name='Ripr Rust Small on GitHub Hosted / Rust gates', status='completed', conclusion='success',
                        steps=[step('Set up job', 1), step('Run actions/checkout@v6', 2), step('Capture checkout observation', 3),
                               step(f'Checkout source v1 commit={COMMIT} tree={TREE}', 4),
                               step(f'Checkout parents v1 {BASE},{HEAD}', 5),
                               step(f'Checkout origin v1 repo={m.REPO} workflow={GENERATION} run=10 attempt=1 job=rust-gates', 6)])
        self.detail = dict(id=10, run_attempt=1, head_sha=HEAD, event='pull_request', workflow_id=m.WORKFLOW_ID,
                           path='.github/workflows/routed-rust.yml', repository={'id': m.REPO_ID},
                           referenced_workflows=[dict(path=f'{m.REPO}/.github/workflows/rust-gates.yml@{GENERATION}',
                                                      sha=GENERATION, ref='refs/pull/42/merge')])
        self.obj = dict(sha=COMMIT, tree={'sha': TREE}, parents=[{'sha': BASE}, {'sha': HEAD}])
        self.caller, self.leaf = CALLER, LEAF
    def get(self, path):
        if path == 'actions/jobs/30': return copy.deepcopy(self.job)
        if path == 'actions/runs/10/attempts/1': return copy.deepcopy(self.detail)
        if path == f'contents/.github/workflows/routed-rust.yml?ref={GENERATION}': return {'sha': self.caller}
        if path in (f'contents/.github/workflows/rust-gates.yml?ref={GENERATION}', f'contents/.github/workflows/rust-test-shards.yml?ref={GENERATION}'): return {'sha': self.leaf}
        if path == f'git/commits/{COMMIT}': return copy.deepcopy(self.obj)
        raise m.Refusal('unexpected API read')


class ReaderTests(unittest.TestCase):
    def test_matching_subject(self):
        self.assertEqual(m.read(API(), **PARAMS)['parents'], [BASE, HEAD])

    def test_matching_shard_subject(self):
        api = API()
        api.job['name'] = 'Ripr Rust Small tests on GitHub Hosted / Rust tests shard 2 of 4'
        api.job['steps'][5]['name'] = api.job['steps'][5]['name'].replace('job=rust-gates', 'job=rust-test-shard')
        api.detail['referenced_workflows'][0]['path'] = f'{m.REPO}/.github/workflows/rust-test-shards.yml@{GENERATION}'
        self.assertEqual(m.read(api, **PARAMS)['job'], 30)

    def test_rejects_missing_failed_stale_forged_and_ambiguous_evidence(self):
        def missing(a): a.job['steps'].pop(3)
        def failed(a): a.job['steps'][2]['conclusion'] = 'failure'
        def failed_checkout(a): a.job['steps'][1]['conclusion'] = 'failure'
        def wrong_order(a): a.job['steps'][3]['number'] = 9
        def duplicate(a): a.job['steps'].append(copy.deepcopy(a.job['steps'][3]))
        def stale_attempt(a): a.job['run_attempt'] = 2
        def wrong_run(a): a.job['run_id'] = 11
        def wrong_job(a): a.job['id'] = 31
        def job_failure(a): a.job['conclusion'] = 'failure'
        def forged_head(a): a.job['steps'][4]['name'] = f'Checkout parents v1 {HEAD},{BASE}'
        def unsupported_parents(a): a.job['steps'][4]['name'] = 'Checkout parents v1 unsupported'
        def forged_tree(a): a.obj['tree']['sha'] = '9' * 40
        def wrong_commit(a): a.obj['sha'] = '9' * 40
        def wrong_workflow(a): a.detail['workflow_id'] = 7
        def wrong_repository(a): a.detail['repository']['id'] = 7
        def missing_ref(a): a.detail['referenced_workflows'] = []
        def duplicate_ref(a): a.detail['referenced_workflows'] *= 2
        def conflicting_ref(a):
            other = copy.deepcopy(a.detail['referenced_workflows'][0])
            other['sha'] = '9' * 40
            other['path'] = other['path'].replace(GENERATION, '9' * 40)
            a.detail['referenced_workflows'].append(other)
        def wrong_caller(a): a.caller = '9' * 40
        def wrong_leaf(a): a.leaf = '9' * 40
        def wrong_origin(a): a.job['steps'][5]['name'] = a.job['steps'][5]['name'].replace('attempt=1', 'attempt=2')
        def wrong_ref(a): a.detail['referenced_workflows'][0]['ref'] = 'refs/pull/43/merge'
        for mutation in (missing, failed, failed_checkout, wrong_order, duplicate, stale_attempt, wrong_run, wrong_job, job_failure, forged_head, unsupported_parents, forged_tree, wrong_commit, wrong_workflow, wrong_repository, missing_ref, duplicate_ref, conflicting_ref, wrong_caller, wrong_leaf, wrong_origin, wrong_ref):
            with self.subTest(mutation=mutation.__name__):
                api = API(); mutation(api)
                with self.assertRaises(m.Refusal): m.read(api, **PARAMS)

    def test_expected_subject_and_blob_are_external_inputs(self):
        for field in ('head', 'base', 'tree', 'caller_blob', 'leaf_blob'):
            with self.subTest(field=field):
                params = dict(PARAMS); params[field] = '9' * 40
                with self.assertRaises(m.Refusal): m.read(API(), **params)


class ProducerTests(unittest.TestCase):
    def test_both_workflows_execute_the_same_observation_on_real_git(self):
        bodies = []
        for name in ('rust-gates.yml', 'rust-test-shards.yml'):
            text = (ROOT / '.github' / 'workflows' / name).read_text()
            block = text.split('      - name: Capture checkout observation\n', 1)[1].split('\n      - name:', 1)[0]
            body = textwrap.dedent(block.split('        run: |\n', 1)[1])
            bodies.append(body)
            self.assertIn('steps.checkout_observation.outputs.commit', text)
        self.assertEqual(*bodies)
        with tempfile.TemporaryDirectory() as directory:
            env = dict(os.environ, GIT_AUTHOR_NAME='test', GIT_AUTHOR_EMAIL='test@example.com', GIT_COMMITTER_NAME='test', GIT_COMMITTER_EMAIL='test@example.com')
            def git(*args, stdin=None):
                return subprocess.run(['git', *args], cwd=directory, env=env, input=stdin, text=True, capture_output=True, check=True).stdout.strip()
            git('init', '-q')
            tree = git('mktree', stdin='')
            base = git('commit-tree', tree, '-m', 'base')
            head = git('commit-tree', tree, '-m', 'head')
            commit = git('commit-tree', tree, '-p', base, '-p', head, '-m', 'merge')
            git('checkout', '-q', '--detach', commit)
            output = Path(directory) / 'output'
            env['GITHUB_OUTPUT'] = str(output)
            subprocess.run(['bash', '-c', bodies[0]], cwd=directory, env=env, check=True)
            self.assertEqual(output.read_text(), f'commit={commit}\ntree={tree}\nparents={base},{head}\n')
            output.unlink()
            # A producer that records the first parent instead of HEAD must not
            # satisfy the actual-checkout oracle.
            wrong = bodies[0].replace("HEAD^{commit}", "HEAD^1^{commit}")
            subprocess.run(['bash', '-c', wrong], cwd=directory, env=env, check=True)
            self.assertNotEqual(output.read_text(), f'commit={commit}\ntree={tree}\nparents={base},{head}\n')
            output.unlink()
            git('checkout', '-q', '--detach', base)
            subprocess.run(['bash', '-c', bodies[0]], cwd=directory, env=env, check=True)
            self.assertEqual(output.read_text(), f'commit={base}\ntree={tree}\nparents=none\n')
            output.unlink()
            subprocess.run(['bash', '-c', bodies[0]], cwd='/tmp', env=env, check=False, capture_output=True)
            self.assertFalse(output.exists())


if __name__ == '__main__':
    unittest.main()
