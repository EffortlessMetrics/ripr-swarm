"""Real CLI controls for exact-head, evidence-bound source-review refutations."""
import base64
import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).with_name('review_disposition.py')
HEAD = 'a' * 40
HEADER = 'RIPR_REVIEW_RESOLUTION_V1\n'
SOURCE = b'pub fn bounded() {\n    return;\n}\n'
BLOB = hashlib.sha1(b'blob ' + str(len(SOURCE)).encode() + b'\0' + SOURCE).hexdigest()
GH = '''import json,os,sys
from pathlib import Path
v=json.loads(Path(os.environ['REVIEW_FIXTURE']).read_text())
e=next(a for a in sys.argv if a.startswith("repos/"))
with open(os.environ['REVIEW_READS'],'a') as f: f.write(e+'\\n')
if e=='repos/o/r/pulls/17/reviews?per_page=100': out=v['reviews']
elif e=='repos/o/r/pulls/17': out=v['pull']
elif e=='repos/o/r/contents/src/lib.rs?ref='+'a'*40:
    if v.get('source_error'): raise SystemExit(1)
    out=v['source']
else: raise SystemExit('unexpected endpoint: '+e)
print(json.dumps(out))
'''


def review(i, body, **kw):
    return dict(id=i, user={'login':'shared-account'}, author_association='MEMBER',
                state='COMMENTED', commit_id=HEAD,
                submitted_at=f'2026-11-13T12:00:{i:02}Z', body=body, **kw)


def fixture():
    negative=review(2, 'REPAIR_REQUIRED\nThe alleged unbounded path needs inspection.')
    record=dict(schema='ripr.review-resolution.v1', repository='o/r', pull_request=17,
        head_sha=HEAD, resolutions=[dict(review_id=2,
        body_sha256=hashlib.sha256(negative['body'].encode()).hexdigest(),
        disposition='refuted', rationale='The cited exact-head function returns without a loop.',
        source_evidence=[dict(path='src/lib.rs',blob_sha=BLOB,start_line=1,end_line=3,
                             excerpt_sha256=hashlib.sha256(SOURCE).hexdigest())])])
    return dict(reviews=[review(1,'SOURCE_REVIEW_READY'),negative,
                        review(3,HEADER+json.dumps(record))],
        pull=dict(number=17,author_association='MEMBER',head=dict(sha=HEAD,repo=dict(full_name='o/r')),
                  base=dict(repo=dict(full_name='o/r')),body=f'Source review: REVIEW_READY {HEAD}'),
        source=dict(type='file',path='src/lib.rs',encoding='base64',sha=BLOB,size=len(SOURCE),
                    content=base64.b64encode(SOURCE).decode()))


def record(v):
    return json.loads(v['reviews'][2]['body'][len(HEADER):])


def set_record(v, r):
    v['reviews'][2]['body']=HEADER+json.dumps(r)


class Resolution(unittest.TestCase):
    def run_cli(self, v, expected, args=(), strict=True):
        with tempfile.TemporaryDirectory(prefix='ripr-resolution-') as d:
            p=Path(d)
            (p/'fixture.json').write_text(json.dumps(v),encoding='utf-8')
            (p/'gh').write_text('#!'+sys.executable+'\n'+GH,encoding='utf-8')
            (p/'gh').chmod(0o755)
            (p/'gh.cmd').write_text(f'@"{sys.executable}" "%~dp0gh" %*\r\n')
            env=dict(os.environ,PATH=d+os.pathsep+os.environ.get('PATH',''),
                     REVIEW_FIXTURE=str(p/'fixture.json'),REVIEW_READS=str(p/'reads'))
            result=subprocess.run([sys.executable,'-I',str(SCRIPT),'--repository','o/r',
                '--pr','17','--head',HEAD,*(['--require-explicit-resolution'] if strict else []),*args],env=env,text=True,capture_output=True,timeout=10)
            self.assertEqual(result.returncode, expected, result.stderr)
            self.assertNotIn('Traceback',result.stderr)
            self.assertNotIn('could not read reviews',result.stderr)
            return result.stdout,(p/'reads').read_text()

    def test_producer_roundtrip(self):
        v=fixture()
        out,reads=self.run_cli(v,0,('--format-resolution','2','--rationale',
            'The exact cited function returns; the loop allegation is a false positive.',
            '--source-path','src/lib.rs','--start-line','1','--end-line','3'))
        self.assertTrue(out.startswith(HEADER))
        self.assertIn('contents/src/lib.rs?ref='+HEAD,reads)
        v['reviews'][2]['body']=out
        self.run_cli(v,0)

    def test_formatter_and_consumer_share_subject_and_rationale_limits(self):
        args=('--format-resolution','2','--source-path','src/lib.rs','--start-line','1','--end-line','3')
        v=fixture();v['pull']['head']['repo']['full_name']='fork/r'
        self.run_cli(v,1,args+('--rationale','The source refutes the allegation.'))
        v=fixture();v['pull']['number']=True
        self.run_cli(v,1,args+('--rationale','The source refutes the allegation.'))
        for rationale in [' '*66000+'x','x'*4001,' ']:
            v=fixture();self.run_cli(v,1,args+('--rationale',rationale))
            r=record(v);r['resolutions'][0]['rationale']=rationale;set_record(v,r)
            self.run_cli(v,1)
        v=fixture()
        out,_=self.run_cli(v,0,args+('--rationale','é'*4000))
        v['reviews'][2]['body']=out;self.run_cli(v,0)

    def test_historical_red_control_can_be_explicitly_retracted(self):
        v=fixture();v['reviews'][1]['body']='REPAIR_REQUIRED\nThe test fails before the repair.'
        r=record(v);r['resolutions'][0]['body_sha256']=hashlib.sha256(v['reviews'][1]['body'].encode()).hexdigest()
        r['resolutions'][0]['rationale']='The quoted failure describes the historical negative control, not this exact-head implementation.'
        r['resolutions'][0]['disposition']='retracted';set_record(v,r)
        self.run_cli(v,0)
        v['reviews'][2]['body']='SOURCE_REVIEW_READY';self.run_cli(v,1)

    def test_real_resolution_preserves_same_head_refutation(self):
        v=fixture()
        self.run_cli(v,0)
        r=record(v);r['resolutions'][0]['disposition']='retracted';set_record(v,r)
        self.run_cli(v,0)
        v['reviews'].reverse();self.run_cli(v,0)

    def test_generic_positivity_cannot_clear(self):
        for state,body in [('COMMENTED','SOURCE_REVIEW_READY'),('APPROVED','')]:
            v=fixture();v['reviews'][2].update(state=state,body=body)
            self.run_cli(v,0,strict=False)  # retained chronology generation
            self.run_cli(v,1)
        v=fixture();v['reviews']=[v['reviews'][0]];self.run_cli(v,0)

    def test_record_is_not_source_attestation(self):
        v=fixture();v['pull']['body']='';v['reviews']=v['reviews'][1:]
        self.run_cli(v,1)

    def test_new_objection_withdraws_resolution(self):
        v=fixture();v['reviews'] += [review(4,'REPAIR_REQUIRED\nA new counterexample remains.'),
                                   review(5,'SOURCE_REVIEW_READY')]
        self.run_cli(v,1)
        r=record(v);n=copy.deepcopy(r['resolutions'][0]);n['review_id']=4
        n['body_sha256']=hashlib.sha256(v['reviews'][3]['body'].encode()).hexdigest()
        r['resolutions'].append(n)
        v['reviews'].append(review(6,HEADER+json.dumps(r)))
        self.run_cli(v,0)

    def test_mutated_finding_stales_resolution(self):
        v=fixture();v['reviews'][1]['body']+=' Additional finding.';self.run_cli(v,1)

    def test_partial_and_retracted_evidence(self):
        for key,value in [('sha','b'*40),('path','other.rs'),('type','symlink'),('size',False),
                          ('encoding','none'),('content','!!!!')]:
            with self.subTest(key=key):
                v=fixture();v['source'][key]=value;self.run_cli(v,1)
        v=fixture();v['source_error']=True;self.run_cli(v,1)

    def test_identity_authority_and_chronology(self):
        for field,value in [('user',{'login':'another-account'}),('user',None),
                            ('author_association','NONE'),('commit_id','b'*40),
                            ('state','DISMISSED'),('submitted_at',None),
                            ('submitted_at','2026-11-13T11:59:00Z')]:
            with self.subTest(field=field,value=value):
                v=fixture();v['reviews'][2][field]=value;self.run_cli(v,1)
        v=fixture();v['reviews'].append(copy.deepcopy(v['reviews'][1]));self.run_cli(v,1)
        v=fixture();v['pull']['head']['sha']='b'*40;self.run_cli(v,1)

    def test_closed_schema_and_subject(self):
        for field,value in [('schema','unknown'),('repository','other/repo'),('pull_request',18),
                            ('pull_request',True),('head_sha','b'*40),('resolutions',[]),('extra',True)]:
            with self.subTest(field=field):
                v=fixture();r=record(v);r[field]=value;set_record(v,r);self.run_cli(v,1)
        for field,value in [('review_id',999),('review_id',True),('body_sha256','0'*64),
                            ('disposition','repaired'),('rationale',' '),('source_evidence',[])]:
            with self.subTest(field=field):
                v=fixture();r=record(v);r['resolutions'][0][field]=value;set_record(v,r);self.run_cli(v,1)
        v=fixture();v['reviews'][2]['body']=v['reviews'][2]['body'].replace('"schema":', '"schema":"x","schema":',1)
        self.run_cli(v,1)

    def test_evidence_binding_and_path_boundaries(self):
        for field,value in [('path','../src/lib.rs'),('path','/src/lib.rs'),('path','src\\lib.rs'),
                            ('blob_sha','b'*40),('excerpt_sha256','0'*64),('start_line',True),
                            ('start_line',0),('end_line',2000),('end_line',4)]:
            with self.subTest(field=field):
                v=fixture();r=record(v);r['resolutions'][0]['source_evidence'][0][field]=value
                set_record(v,r);self.run_cli(v,1)

    def test_resolution_errors_cannot_be_cleared_by_positivity(self):
        for body in ['RIPR_REVIEW_RESOLUTION_V2\n{}', HEADER+'{}']:
            v=fixture();v['reviews'].append(review(4,body))
            v['reviews'].append(review(5,'SOURCE_REVIEW_READY'))
            self.run_cli(v,1)
        # Editing a resolution invalidates its authority; a fresh marker is not a repair.
        v=fixture();r=record(v);r['resolutions'][0]['source_evidence'][0]['excerpt_sha256']='0'*64
        set_record(v,r);v['reviews'].append(review(4,'SOURCE_REVIEW_READY'))
        self.run_cli(v,1)
        # Correcting the record restores the independently checked evidence binding.
        r['resolutions'][0]['source_evidence'][0]['excerpt_sha256']=hashlib.sha256(SOURCE).hexdigest()
        set_record(v,r);self.run_cli(v,0)

    def test_failed_source_reads_are_not_repeated(self):
        v=fixture();v['source_error']=True
        v['reviews'].append(review(4,v['reviews'][2]['body']))
        _,reads=self.run_cli(v,1)
        self.assertEqual(reads.count('/contents/'),1)

    def test_unique_source_read_budget_is_bounded(self):
        v=fixture();r=record(v);v['reviews']=v['reviews'][:2]
        for i in range(17):
            entry=copy.deepcopy(r)
            entry['resolutions'][0]['source_evidence'][0]['path']=f'missing-{i}.rs'
            v['reviews'].append(review(i+3,HEADER+json.dumps(entry)))
        _,reads=self.run_cli(v,1)
        self.assertEqual(reads.count('/contents/'),16)

    def test_formatter_refuses_malformed_source_without_posting(self):
        for source in [None,{},dict(content='x'*1500001)]:
            v=fixture();v['source']=source
            _,reads=self.run_cli(v,1,('--format-resolution','2','--rationale','Source refutation',
                '--source-path','src/lib.rs','--start-line','1','--end-line','3'))
            self.assertEqual(len(reads.splitlines()),3)

    def test_existing_native_veto_is_separate(self):
        v=fixture();v['reviews'].append(review(4,''));v['reviews'][-1]['state']='CHANGES_REQUESTED'
        self.run_cli(v,1)


if __name__=='__main__':
    if sys.argv[1:]==['--self-test']: sys.argv=sys.argv[:1]
    unittest.main()
