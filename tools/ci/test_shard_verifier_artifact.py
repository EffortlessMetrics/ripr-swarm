"""Disposable command-entry/ZIP controls; no hosted credentials or artifact execution."""
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest
import warnings
import zipfile

SCRIPT = Path(__file__).with_name("shard-verifier-artifact.py").resolve()
ROOT = SCRIPT.parents[2]
SPEC = importlib.util.spec_from_file_location("verifier_artifact", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)
SHA = "1" * 40


class ExecutableArtifact(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.checkout = self.root / "checkout"
        (self.checkout / "tools/ci").mkdir(parents=True)
        shutil.copyfile(SCRIPT, self.checkout / "tools/ci" / SCRIPT.name)
        self.runner = self.root / "runner"
        self.runner.mkdir()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.marker = self.root / "marker"
        self.expected = MODULE.identity("owner/repo", 27, 2, SHA)
        self.run = dict(id=27, run_attempt=2, repository=dict(id=42, full_name="owner/repo"),
                        head_repository=dict(id=42), head_sha="2" * 40)
        self.artifacts = []
        self.responses = {}
        self.payload = (f"#!{sys.executable}\n" + '''import json,os,sys
from pathlib import Path
with open(os.environ['MARKER'], 'a') as out:
 out.write(json.dumps(dict(path=sys.argv[0], args=sys.argv[1:], token=os.environ.get('GH_TOKEN'), github_token=os.environ.get('GITHUB_TOKEN'))) + '\\n')
Path('target/nextest/ci/shards-summary.txt').write_text('fixture summary\\n')
sys.exit(int(os.environ.get('STUB_EXIT', '0')))
''').encode()
        self._stub("git", '#!/bin/sh\nprintf "%s\\n" "' + SHA + '"\n')
        self._stub("gh", f"#!{sys.executable}\n" + '''import json,os,sys
from pathlib import Path
assert sys.argv[1:4] == ['api','--hostname','github.com']
endpoint=sys.argv[4]
root=Path(os.environ['FIXTURE'])
with (root/'requests').open('a') as log: log.write(endpoint+'\\n')
data=json.loads((root/'responses.json').read_text())
if data.get('error_endpoint') == endpoint:
 print(data['error_message'],file=sys.stderr)
 sys.exit(1)
if endpoint.endswith('/zip'):
 sys.stdout.buffer.write(Path(data[endpoint]).read_bytes())
else:
 result=data[endpoint]
 if endpoint.endswith('artifacts?per_page=100'): assert sys.argv[5:] == ['--paginate','--slurp']
 if endpoint.endswith('/runs/27') and (root/'requests').read_text().count(endpoint+'\\n') > 1:
  result=data.get('second_run',result)
 print(json.dumps(result))
''')
        self.env = dict(os.environ, PATH=str(self.bin) + os.pathsep + os.environ['PATH'],
                        MARKER=str(self.marker), FIXTURE=str(self.root), GH_TOKEN='read-only-fixture',
                        GITHUB_TOKEN='second-read-only-fixture', GITHUB_REPOSITORY='owner/repo',
                        GITHUB_RUN_ID='27', GITHUB_RUN_ATTEMPT='2', RUNNER_TEMP=str(self.runner),
                        ARTIFACT_PREFIX='proof', GITHUB_STEP_SUMMARY=str(self.root/'summary'))

    def _stub(self, name, body):
        path = self.bin / name
        path.write_text(body)
        path.chmod(0o700)

    def artifact(self, attempt=2, members=None, receipt_patch=None, name=None, artifact_id=None):
        artifact_id = artifact_id or 100 + len(self.artifacts)
        receipt = dict(self.expected, run_attempt=attempt, schema=1,
                       binary_sha256=hashlib.sha256(self.payload).hexdigest())
        receipt.update(receipt_patch or {})
        members = members if members is not None else [('xtask', self.payload, stat.S_IFREG | 0o644),
                                      ('identity.json', json.dumps(receipt).encode(), stat.S_IFREG | 0o644)]
        archive = self.root / f'{artifact_id}.zip'
        with warnings.catch_warnings():
            warnings.simplefilter('ignore', UserWarning)
            with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as output:
                for filename, data, mode in members:
                    member = zipfile.ZipInfo(filename)
                    member.create_system = 3
                    member.external_attr = mode << 16
                    output.writestr(member, data)
        metadata = dict(id=artifact_id, name=name or f'proof-xtask-{attempt}', expired=False,
                        size_in_bytes=archive.stat().st_size, digest='sha256:'+MODULE.digest(archive),
                        workflow_run=dict(id=27, head_sha=self.run['head_sha'], repository_id=42, head_repository_id=42))
        self.artifacts.append(metadata)
        self.responses[f'repos/owner/repo/actions/artifacts/{artifact_id}'] = metadata
        self.responses[f'repos/owner/repo/actions/artifacts/{artifact_id}/zip'] = str(archive)
        return metadata

    def execute(self, workflow=True):
        self.responses['repos/owner/repo/actions/runs/27'] = self.run
        self.responses['repos/owner/repo/actions/runs/27/artifacts?per_page=100'] = [dict(
            total_count=len(self.artifacts), artifacts=self.artifacts)]
        (self.root / 'responses.json').write_text(json.dumps(self.responses))
        if workflow:
            source = (ROOT / '.github/workflows/rust-test-shards.yml').read_text()
            step = source.split('      - name: Check the shards are a complete, disjoint, green cover\n')[1]
            lines = step.split('        run: |\n')[1].split('\n\n      - name:')[0].splitlines()
            command = '\n'.join(line[10:] for line in lines)
            args = ['bash', '-e', '-o', 'pipefail', '-c', command]
        else:
            args = [sys.executable, str(SCRIPT), 'launch', '--repository', 'owner/repo', '--run-id', '27',
                    '--attempt', '2', '--sha', SHA, '--prefix', 'proof', '--runner-temp', str(self.runner),
                    '--provenance', 'target/nextest/ci/verifier-artifact.json', '--', 'verify-test-shards']
        result = subprocess.run(args, cwd=self.checkout, env=self.env, text=True, capture_output=True, timeout=10)
        self.assertEqual(list(self.runner.iterdir()), [], 'temporary executable archive leaked')
        return result

    def refused(self, expected_error):
        result = self.execute()
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(expected_error, result.stderr)
        self.assertFalse(self.marker.exists(), 'refusal executed an artifact')

    def test_workflow_launches_single_exact_artifact_and_retains_identity(self):
        self.artifact()
        result = self.execute()
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = [json.loads(row) for row in self.marker.read_text().splitlines()]
        self.assertEqual(len(calls), 1)
        self.assertTrue(Path(calls[0]['path']).is_relative_to(self.runner))
        self.assertEqual(calls[0]['args'][0], 'verify-test-shards')
        self.assertIsNone(calls[0]['token'])
        self.assertIsNone(calls[0]['github_token'])
        provenance = json.loads((self.checkout/'target/nextest/ci/verifier-artifact.json').read_text())
        self.assertEqual((provenance['artifact_id'], provenance['producer_attempt'], provenance['run_attempt']), (100,2,2))
        self.assertIn('/artifacts/100/zip', (self.root/'requests').read_text())

    def test_earlier_reuse_and_numeric_attempt_order(self):
        self.expected['run_attempt'] = self.run['run_attempt'] = 11
        self.env['GITHUB_RUN_ATTEMPT'] = '11'
        for attempt in [2, 10, 1]: self.artifact(attempt)
        result = self.execute()
        self.assertEqual(result.returncode, 0, result.stderr)
        provenance = json.loads((self.checkout/'target/nextest/ci/verifier-artifact.json').read_text())
        self.assertEqual((provenance['producer_attempt'], provenance['run_attempt']), (10,11))

    def test_shadow_in_candidate_checkout_is_ignored(self):
        self.artifact()
        shadow = self.checkout/'xtask-binary/zz/xtask'
        shadow.parent.mkdir(parents=True)
        shadow.write_text('#!/bin/sh\nexit 99\n')
        self.assertEqual(self.execute().returncode, 0)
        self.assertEqual(shadow.read_text(), '#!/bin/sh\nexit 99\n')

    def test_no_download_cannot_use_preseeded_shadow(self):
        shadow = self.checkout/'xtask-binary/zz/xtask'
        shadow.parent.mkdir(parents=True)
        shadow.write_bytes(self.payload)
        self.refused('no executable artifact')

    def test_future_attempt_refuses(self):
        self.artifact(1)
        self.artifact(3)
        self.refused('future executable artifact')

    def test_noncanonical_suffixes(self):
        for suffix in ['0','01','-1','one','2.0','2-old','2\n']:
            with self.subTest(suffix=suffix):
                self.artifacts.clear()
                self.artifact(name='proof-xtask-'+suffix)
                self.refused('noncanonical positive identity')

    def test_duplicate_attempt_refuses(self):
        self.artifact()
        self.artifact()
        self.refused('ambiguous executable artifact attempt')

    def test_missing_latest_member_never_falls_back(self):
        self.artifact(1)
        self.artifact(2, members=[('identity.json',b'{}',stat.S_IFREG)])
        self.refused('exactly xtask and identity.json')

    def test_member_ambiguity_and_substitution(self):
        valid_receipt = json.dumps(dict(self.expected,schema=1,binary_sha256=hashlib.sha256(self.payload).hexdigest())).encode()
        normal = [('xtask',self.payload,stat.S_IFREG), ('identity.json',valid_receipt,stat.S_IFREG)]
        variants = [normal + [('nested/xtask',self.payload,stat.S_IFREG)], normal + [normal[0]],
                    [('xtask/',b'x',stat.S_IFDIR),normal[1]],
                    [('xtask',self.payload,stat.S_IFLNK),normal[1]],
                    [normal[0],('identity.json',valid_receipt,stat.S_IFLNK)],
                    [('xtask',self.payload,stat.S_IFIFO),normal[1]],
                    [('../xtask',self.payload,stat.S_IFREG),normal[1]]]
        for members in variants:
            with self.subTest(names=[m[0] for m in members]):
                self.artifacts.clear()
                self.artifact(members=members)
                result = self.execute()
                self.assertNotEqual(result.returncode,0)
                self.assertFalse(self.marker.exists())

    def test_wrong_producer_identity(self):
        for key,value in [('repository','wrong/repo'),('run_id',28),('run_attempt',1),('checkout_sha','3'*40),('run_id',True),('binary_sha256','0'*64)]:
            with self.subTest(field=key):
                self.artifacts.clear()
                self.artifact(receipt_patch={key:value})
                result=self.execute()
                self.assertNotEqual(result.returncode,0)
                self.assertFalse(self.marker.exists())

    def test_wrong_run_metadata(self):
        self.artifact()['workflow_run']['head_sha'] = '9'*40
        self.refused('wrong artifact run/source')

    def test_digest_mismatch(self):
        self.artifact()['digest'] = 'sha256:'+'0'*64
        self.refused('artifact digest mismatch')

    def test_missing_digest(self):
        self.artifact()['digest'] = None
        self.refused('missing artifact digest')

    def test_expired_latest_never_falls_back(self):
        self.artifact(1)
        self.artifact(2)['expired'] = True
        self.refused('selected executable artifact expired')

    def test_attempt_superseded_before_launch(self):
        self.artifact()
        self.responses['second_run'] = dict(self.run,run_attempt=3)
        self.refused('run superseded before launch')

    def test_artifact_metadata_changed(self):
        artifact=self.artifact()
        self.responses['repos/owner/repo/actions/artifacts/100'] = dict(artifact, name='different')
        self.refused('artifact metadata changed')

    def test_native_failure_propagates_and_cleans_up(self):
        self.artifact()
        self.env['STUB_EXIT']='42'
        result=self.execute()
        self.assertEqual(result.returncode,42,result.stderr)
        self.assertEqual(len(self.marker.read_text().splitlines()),1)

    def test_unsafe_runner_temp(self):
        self.artifact()
        inside=self.checkout/'temp'; inside.mkdir()
        self.env['RUNNER_TEMP']=str(inside)
        self.refused('runner temp must be outside checkout')
        link=self.root/'linked';link.symlink_to(self.runner,target_is_directory=True)
        self.env['RUNNER_TEMP']=str(link)
        self.refused('invalid runner temporary directory')

    def test_retain_command_then_real_zip_roundtrip(self):
        binary=self.root/'built-xtask'; binary.write_bytes(self.payload)
        destination=self.root/'producer'
        result=subprocess.run([sys.executable,str(SCRIPT),'retain','--repository','owner/repo','--run-id','27',
            '--attempt','2','--sha',SHA,'--binary',str(binary),'--destination',str(destination)],capture_output=True,text=True)
        self.assertEqual(result.returncode,0,result.stderr)
        self.artifact(members=[(p.name,p.read_bytes(),stat.S_IFREG) for p in destination.iterdir()])
        self.assertEqual(self.execute().returncode,0)
        with self.assertRaises(FileExistsError): MODULE.retain(binary,destination,self.expected)
        link=self.root/'link'; link.symlink_to(binary)
        with self.assertRaisesRegex(ValueError,'regular'): MODULE.retain(link,self.root/'other',self.expected)

    def test_extraction_refuses_existing_directory_or_symlink(self):
        artifact=self.artifact()
        archive=self.root/'100.zip'
        directory=self.root/'already'; directory.mkdir()
        link=self.root/'link';link.symlink_to(directory,target_is_directory=True)
        for dest in [directory,link]:
            with self.assertRaises(FileExistsError): MODULE.unpack(archive,dest,artifact,self.expected)
        self.assertEqual(list(directory.iterdir()),[])

    def test_download_errors_do_not_log_signed_urls(self):
        self.artifact()
        self.responses['error_endpoint']='repos/owner/repo/actions/artifacts/100/zip'
        self.responses['error_message']='403 https://storage.invalid/payload?sig=BEARER_SECRET'
        result=self.execute()
        self.assertNotEqual(result.returncode,0)
        self.assertIn('GitHub artifact read failed',result.stderr)
        self.assertNotIn('BEARER_SECRET',result.stderr+result.stdout)
        self.assertNotIn('storage.invalid',result.stderr+result.stdout)
        self.assertFalse(self.marker.exists())

    def test_zip_permission_bits_without_type_are_regular(self):
        receipt=json.dumps(dict(self.expected,schema=1,binary_sha256=hashlib.sha256(self.payload).hexdigest())).encode()
        self.artifact(members=[('xtask',self.payload,0o644),('identity.json',receipt,0o644)])
        self.assertEqual(self.execute().returncode,0)

    def test_digest_streams_multiple_chunks(self):
        payload=bytes(range(256))*1025
        path=self.root/'multi-chunk';path.write_bytes(payload)
        self.assertEqual(MODULE.digest(path),hashlib.sha256(payload).hexdigest())

    def test_inventory_completeness_and_duplicate_ids(self):
        artifact=self.artifact()
        for pages in [[dict(total_count=2,artifacts=[artifact])],
                      [dict(total_count=2,artifacts=[artifact,artifact])]]:
            with self.assertRaises(ValueError): MODULE.select(pages,self.expected,'proof',self.run)


if __name__ == '__main__':
    unittest.main()
