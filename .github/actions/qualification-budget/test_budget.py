"""Native deadline/custody controls, with fixtures only and no network."""
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
spec = importlib.util.spec_from_file_location('budget', HERE / 'budget.py')
budget = importlib.util.module_from_spec(spec)
spec.loader.exec_module(budget)
START = 1793952000
ENV = dict(GITHUB_EVENT_NAME='merge_group', GITHUB_REPOSITORY='EffortlessMetrics/ripr-swarm',
           GITHUB_RUN_ID='42', GITHUB_RUN_ATTEMPT='1', GITHUB_SHA='a' * 40,
           GITHUB_WORKFLOW_SHA='b' * 40, BUDGET_SUBJECT='a' * 40)
META = dict(id=42, run_attempt=1, event='merge_group', head_sha='a' * 40,
            repository=dict(full_name=ENV['GITHUB_REPOSITORY']),
            run_started_at='2026-11-06T08:00:00Z')


def python(args, env):
    return subprocess.run([sys.executable, *args], env=env, capture_output=True, timeout=5)


def job(source, name):
    tail = source.split('\njobs:\n', 1)[1].split('  ' + name + ':\n', 1)[1]
    lines = []
    for line in tail.splitlines():
        if line.startswith('  ') and not line.startswith('   ') and not line.startswith('  #'):
            break
        lines.append(line)
    return '\n'.join(lines)


def loader(source):
    tail = source.split('      run: |\n', 1)[1]
    return '\n'.join(line[8:] for line in tail.splitlines() if line.strip())


class Deadline(unittest.TestCase):
    def receipt(self, now=START):
        return budget.issue(copy.deepcopy(META), ENV, now)

    def test_native_timestamp_and_whole_budget_are_exact(self):
        self.assertEqual(int(budget.datetime.strptime(META['run_started_at'], '%Y-%m-%dT%H:%M:%SZ').replace(tzinfo=budget.timezone.utc).timestamp()), START)
        receipt = self.receipt()
        self.assertEqual(budget.verify(receipt, ENV, START, self.receipt()), 1200)
        self.assertEqual(budget.verify(receipt, ENV, START + 1199, self.receipt()), 1)
        with self.assertRaises(ValueError):
            budget.verify(receipt, ENV, START + 1200, self.receipt())

    def test_sequential_children_never_receive_new_time(self):
        receipt = self.receipt()
        for elapsed in [0, 27, 48, 600, 1199]:
            self.assertEqual(self.receipt(START + elapsed), receipt)
            self.assertEqual(budget.verify(receipt, ENV, START + elapsed, self.receipt()), 1200 - elapsed)

    def test_renewing_both_timestamps_cannot_replace_the_artifact_origin(self):
        original = self.receipt()
        renewed = json.loads(original)
        renewed['started_epoch'] += 600
        renewed['deadline_epoch'] += 600
        with self.assertRaises(ValueError):
            budget.verify(budget.canonical(renewed), ENV, START + 600, original)

    def test_wrong_native_origin_cannot_issue(self):
        for key, value in [('id', 43), ('run_attempt', 2), ('event', 'pull_request'),
                           ('head_sha', 'c' * 40), ('run_started_at', ''),
                           ('run_started_at', '2026-11-06T08:00:01Z'), ('id', True),
                           ('repository', {'full_name': 'other/repository'})]:
            with self.subTest(key=key, value=value):
                wrong = copy.deepcopy(META)
                wrong[key] = value
                with self.assertRaises(ValueError):
                    budget.issue(wrong, ENV, START)

    def test_rebound_generation_subject_run_attempt_and_repository_refuse(self):
        receipt = self.receipt()
        for key, value in [('GITHUB_RUN_ID', '43'), ('GITHUB_RUN_ATTEMPT', '2'),
                           ('GITHUB_WORKFLOW_SHA', 'c' * 40), ('GITHUB_REPOSITORY', 'o/r'),
                           ('BUDGET_SUBJECT', 'c' * 40), ('GITHUB_EVENT_NAME', 'pull_request')]:
            with self.subTest(key=key):
                with self.assertRaises(ValueError):
                    budget.verify(receipt, dict(ENV, **{key: value}), START, self.receipt())

    def test_extended_noncanonical_missing_and_duplicate_receipts_refuse(self):
        value = json.loads(self.receipt())
        mutations = ['', '{}', self.receipt() + '\n', self.receipt()[:-1] + ',"version":1}']
        for key, replacement in [('deadline_epoch', START + 1201), ('started_epoch', True),
                                 ('version', True), ('version', 2), ('extra', 'field')]:
            mutations.append(budget.canonical(dict(value, **{key: replacement})))
        for receipt in mutations:
            with self.subTest(receipt=receipt):
                with self.assertRaises((ValueError, json.JSONDecodeError)):
                    budget.verify(receipt, ENV, START, self.receipt())

    def test_expired_cli_leaves_no_success_outputs(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'outputs'
            checker = Path(directory) / 'budget.py'
            checker.write_bytes((HERE / 'budget.py').read_bytes())
            (Path(directory) / 'receipt.json').write_text(self.receipt())
            env = dict(os.environ, **ENV, BUDGET_RECEIPT=self.receipt(), BUDGET_ISSUE='false', GITHUB_OUTPUT=str(output))
            done = python(['-c', 'import runpy,time;time.time=lambda: '+str(START+1200)+';runpy.run_path('+repr(str(checker))+',run_name="__main__")'], env)
            self.assertNotEqual(done.returncode, 0)
            self.assertFalse(output.exists())

    def test_actual_source_free_loader_rejects_changed_verifier_before_execution(self):
        program = loader((HERE.parent / 'consume-qualification-budget/action.yml').read_text())
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'qualification-budget').mkdir()
            script = root / 'qualification-budget/budget.py'
            marker = root / 'executed'
            script.write_text('from pathlib import Path\nPath(' + repr(str(marker)) + ').touch()\n')
            env = dict(os.environ, BUDGET_SOURCE=str(script), BUDGET_VERIFIER_SHA256=hashlib.sha256((HERE / 'budget.py').read_bytes()).hexdigest())
            done = python(['-c', program], env)
            self.assertNotEqual(done.returncode, 0)
            self.assertFalse(marker.exists())

    def test_actual_issuer_and_source_free_consumer_conserve_exact_artifact(self):
        program = loader((HERE.parent / 'consume-qualification-budget/action.yml').read_text())
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            metadata = root / 'run.json'
            metadata.write_text(json.dumps(META))
            output = root / 'issuer.outputs'
            env = dict(os.environ, **ENV, RUNNER_TEMP=directory, BUDGET_RECEIPT='', BUDGET_ISSUE='true',
                       BUDGET_METADATA=str(metadata), GITHUB_OUTPUT=str(output))
            command = 'import time,runpy;time.time=lambda: '+str(START)+';runpy.run_path('+repr(str(HERE/'budget.py'))+',run_name="__main__")'
            done = python(['-c', command], env)
            self.assertEqual(done.returncode, 0, done.stderr)
            values = dict(line.split('=', 1) for line in output.read_text().splitlines())
            self.assertEqual(values['receipt'], self.receipt())
            staged = root / 'qualification-budget'
            self.assertEqual((staged / 'action.yml').read_bytes(), (HERE.parent / 'consume-qualification-budget/action.yml').read_bytes())
            self.assertEqual((staged / 'receipt.json').read_text(), values['receipt'])
            self.assertEqual(hashlib.sha256((staged / 'budget.py').read_bytes()).hexdigest(), values['verifier_sha256'])
            for elapsed, expected in [(1199, 0), (1200, 1)]:
                consumer_output = root / f'consumer-{elapsed}.outputs'
                consumer_env = dict(env, BUDGET_SOURCE=str(staged/'budget.py'), BUDGET_ISSUE='false',
                                    BUDGET_RECEIPT=values['receipt'], BUDGET_VERIFIER_SHA256=values['verifier_sha256'],
                                    GITHUB_OUTPUT=str(consumer_output))
                done = python(['-c', 'import time;time.time=lambda: '+str(START+elapsed)+'\n'+program], consumer_env)
                self.assertEqual(done.returncode == 0, expected == 0, done.stderr)
                if expected == 0:
                    self.assertIn('remaining_seconds=1\n', consumer_output.read_text())
                else:
                    self.assertFalse(consumer_output.exists())

    def test_every_callable_job_consumes_the_same_artifact_and_deadline(self):
        names = {'rust-gates': ['rust-gates'], 'ci': ['source-identity', 'release-proof', 'rust-only-features', 'msrv', 'ci-result'],
                 'python-wheel-qualification': ['applicability', 'source-contract', 'build-wheel', 'consume'],
                 'windows-subset-qualification': ['windows-advisory-subset'], 'printed-command-paste': ['paste'], 'vscode-e2e': ['vscode']}
        for name, jobs in names.items():
            source = (ROOT / f'.github/workflows/{name}.yml').read_text()
            for key in ['qualification_budget', 'budget_artifact_id', 'budget_verifier_sha256']:
                self.assertIn('      ' + key + ':\n', source)
            for selected in jobs:
                with self.subTest(workflow=name, job=selected):
                    body = job(source, selected)
                    self.assertEqual(body.count('uses: ./target/qualification-budget'), 2)
                    self.assertEqual(body.count('receipt: ${{ inputs.qualification_budget }}'), 2)
                    self.assertEqual(body.count('verifier-sha256: ${{ inputs.budget_verifier_sha256 }}'), 2)
                    self.assertEqual(body.count('artifact-ids: ${{ inputs.budget_artifact_id }}'), 1)
                    self.assertIn('Complete within native qualification deadline', body)
                    self.assertNotIn("issue: 'true'", body)
        consume = job((ROOT / '.github/workflows/python-wheel-qualification.yml').read_text(), 'consume')
        self.assertNotIn('actions/checkout@', consume, 'wheel consumption stays source-checkout-free')


if __name__ == '__main__':
    unittest.main()
