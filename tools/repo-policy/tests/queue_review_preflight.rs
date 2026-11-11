//! #8082/#14113: early admitted-H queue review, retaining fresh terminal reads.
#![cfg(target_os = "linux")]

use std::process::Command;

const CONTROLS: &str = r#"
import copy, json, os, pathlib, subprocess, sys, tempfile, unittest
p=pathlib.Path(sys.argv[1]);H,G='b'*40,'c'*40
REPOSITORY='EffortlessMetrics/ripr-swarm'
class ReviewPreflightControls(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory(prefix='ripr-review-preflight-');self.addCleanup(self.tmp.cleanup)
  self.root=pathlib.Path(self.tmp.name);self.workflow=(p.parents[3]/'.github/workflows/routed-rust.yml').read_text()
  self.trusted=self.root/'.admission-trusted';action=self.trusted/'.github/actions/pr-candidate-admission';action.mkdir(parents=True)
  (action/'review_disposition.py').write_bytes((p.parent/'review_disposition.py').read_bytes())
  self.env=dict(os.environ,GIT_CONFIG_NOSYSTEM='1',GIT_CONFIG_GLOBAL='/dev/null',
    GIT_AUTHOR_NAME='fixture',GIT_AUTHOR_EMAIL='fixture@example.test',GIT_COMMITTER_NAME='fixture',GIT_COMMITTER_EMAIL='fixture@example.test')
  for args in [('init','-q'),('add','.'),('commit','-qm','reviewed reader')]:
   subprocess.run(['git',*args],cwd=self.trusted,env=self.env,check=True,capture_output=True,timeout=5)
  sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=self.trusted,text=True,timeout=5).strip()
  self.env.update(GITHUB_WORKSPACE=str(self.root),TRUSTED_ADMISSION_SHA=sha,REPOSITORY=REPOSITORY,
    PR_NUMBER='42',PR_HEAD=H,EXPECTED_HEAD=H,EXPECTED_QUEUE_ENTRY='MQE_42',EXPECTED_REQUEST='12-1',
    GITHUB_RUN_ID='12',GITHUB_RUN_ATTEMPT='1',EVENT_NAME='merge_group',GROUP_HEAD_SHA=G,
    GH_TOKEN='fixture-not-a-credential',FIXTURE_STATE=str(self.root/'state.json'),FIXTURE_CALLS=str(self.root/'calls'))
  self.env['PATH']=str(self.root)+os.pathsep+self.env['PATH']
  gh=self.root/'gh';gh.write_text('''#!/usr/bin/env python3
import json,os,pathlib,sys
a=sys.argv[1:];s=json.loads(pathlib.Path(os.environ['FIXTURE_STATE']).read_text())
with open(os.environ['FIXTURE_CALLS'],'a') as f:f.write(json.dumps(a)+'\\n')
if any('/reviews?' in x for x in a):
 assert '--paginate' in a
 for page in s['reviews']:print(json.dumps(page))
 sys.exit(1 if s.get('review_error') else 0)
if a[1]=='graphql':
 if s.get('thread_error'):sys.exit(1)
 cursor=next((x.split('=',1)[1] for x in a if x.startswith('cursor=')), 'null')
 i=0 if cursor=='null' else int(cursor)
 nodes=[{'isResolved':v,'comments':{'nodes':[{'url':'https://example.test/thread'}]}} for v in s['threads'][i]]
 pr={'headRefOid':s['head'],'reviewDecision':s.get('decision'),'mergeQueueEntry':{'id':s['entry']},
     'reviewThreads':{'nodes':nodes,'pageInfo':{'hasNextPage':i+1<len(s['threads']),'endCursor':str(i+1)}}}
 print(json.dumps({'data':{'repository':{'pullRequest':pr}}}));sys.exit(0)
if any('/pulls/' in x for x in a):
 if s.get('pull_error'):sys.exit(1)
 if '--include' in a:sys.stdout.write('HTTP/2.0 200 OK\\nContent-Type: application/json\\r\\n\\r\\n')
 print(json.dumps({'number':42,'state':'open','draft':False,'author_association':'MEMBER','body':s.get('body',''),
  'head':{'sha':s['head'],'repo':{'full_name':os.environ['REPOSITORY']}},
  'base':{'ref':'main','repo':{'full_name':os.environ['REPOSITORY']}},'labels':[{'name':'merge-ready'}]}));sys.exit(0)
run={'id':12,'run_attempt':s.get('attempt',1),'workflow_id':99,'run_number':7,'head_sha':os.environ['GROUP_HEAD_SHA'],
     'event':'merge_group','display_title':'Merge queue '+os.environ['GROUP_HEAD_SHA'],'repository':{'full_name':os.environ['REPOSITORY']}}
if any('/actions/runs/12' in x for x in a):print(json.dumps(run));sys.exit(0)
if any('/actions/workflows/99/runs?' in x for x in a):print(json.dumps({'total_count':1,'workflow_runs':[run]}));sys.exit(0)
raise AssertionError(a)
''');gh.chmod(0o700)
  self.state={'head':H,'entry':'MQE_42','reviews':[[self.review('SOURCE_REVIEW_READY')]],'threads':[[True]]}
 def review(self,body,head=H,who='reviewer',order=1):
  return {'id':order,'user':{'login':who},'author_association':'MEMBER','state':'COMMENTED',
          'commit_id':head,'submitted_at':f'2026-11-11T01:00:0{order}Z','body':body}
 def step(self,name):
  marker='      - name: '+name+'\n'
  return self.workflow.split(marker,1)[1].split('\n      - ',1)[0] if marker in self.workflow else ''
 def shell(self,name):
  body=self.step(name)
  if not body:return ''
  return '\n'.join(line[10:] for line in body.split('        run: |\n',1)[1].splitlines() if line.startswith('          '))
 def run_shell(self,script):
  (self.root/'state.json').write_text(json.dumps(self.state))
  return subprocess.run(['bash','-e','-c',script],cwd=self.root,env=self.env,text=True,capture_output=True,timeout=10)
 def early(self):
  script=self.shell('Require source review disposition at request time')+'\n'+self.shell('Refuse unresolved review threads at request time')
  return self.run_shell(script+'\nprintf heavy-work-would-start')
 def late(self):return self.run_shell(self.shell('Recheck source review disposition for the admitted head'))
 def eligibility(self):return self.run_shell(self.shell('Recheck initially admitted eligibility'))
 def test_missing_or_wrong_head_review_stops_before_heavy_work(self):
  for pages in [[],[[self.review('SOURCE_REVIEW_READY',head='9'*40)]],[[self.review('REPAIR_REQUIRED')]]]:
   with self.subTest(pages=pages):
    self.state['reviews']=pages;r=self.early()
    self.assertNotEqual(r.returncode,0,r.stdout);self.assertNotIn('heavy-work-would-start',r.stdout)
 def test_workflow_wires_both_events_to_admitted_h_and_retains_final_reads(self):
  admission=self.workflow.split('\n  admission:\n',1)[1].split('\n  rust-cx43:\n',1)[0]
  for name in ['Require source review disposition at request time','Refuse unresolved review threads at request time']:
   body=self.step(name);self.assertIn(name,admission)
   self.assertIn("if: (github.event_name == 'pull_request' || github.event_name == 'merge_group') && steps.candidate.outputs.status == 'admitted'",body)
   self.assertIn('PR_NUMBER: ${{ steps.entry.outputs.pr }}',body)
  early=self.step('Require source review disposition at request time')
  self.assertIn('PR_HEAD: ${{ steps.entry.outputs.head }}',early)
  self.assertIn('python3 -I .admission-trusted/.github/actions/pr-candidate-admission/review_disposition.py',early)
  self.assertIn('--head "$PR_HEAD"',early)
  self.assertIn('--head "$EXPECTED_HEAD"',self.step('Recheck source review disposition for the admitted head'))
  for name in ['Recheck source review disposition for the admitted head','Recheck initially admitted eligibility']:
   self.assertIn("github.event_name == 'merge_group' || (github.event_name == 'pull_request'",self.step(name))
   self.assertIn("needs.admission.outputs.status == 'admitted'",self.step(name))
  for job in ['rust-cx43','rust-cpx42','rust-cx53','rust-github']:
   header=self.workflow.split('\n  '+job+':\n',1)[1].split('    with:\n',1)[0]
   self.assertIn('    needs: [admission,',header)
   self.assertIn("needs.admission.result == 'success'",header)
 def test_late_neutral_negative_and_wrong_head_reread_is_not_cached(self):
  for late in [self.review('',order=2),self.review('REPAIR_REQUIRED',order=2),self.review('SOURCE_REVIEW_READY',head='9'*40,order=2)]:
   with self.subTest(late=late):
    self.state['reviews']=[[self.review('SOURCE_REVIEW_READY')]]
    self.assertEqual(self.early().returncode,0)
    self.state['reviews'].append([late]);self.assertNotEqual(self.late().returncode,0)
  # Preserve the existing description carrier's neutral-reply behavior too.
  self.state['reviews']=[[self.review('',order=2)]];self.state['body']='Source review: REVIEW_READY '+H
  self.assertEqual(self.early().returncode,0);self.assertEqual(self.late().returncode,0)
 def test_review_pages_and_transport_failures_are_complete_and_fail_closed(self):
  self.state['reviews']=[[],[self.review('SOURCE_REVIEW_READY')]]
  self.assertEqual(self.early().returncode,0);self.assertEqual(self.late().returncode,0)
  self.state['reviews'].append([self.review('REPAIR_REQUIRED',who='other',order=2)])
  self.assertNotEqual(self.early().returncode,0);self.assertNotEqual(self.late().returncode,0)
  self.state['reviews']=[[self.review('SOURCE_REVIEW_READY')]];self.state['review_error']=True
  self.assertNotEqual(self.early().returncode,0);self.assertNotEqual(self.late().returncode,0)
 def test_thread_pagination_and_early_warning_keep_terminal_authority(self):
  self.state['threads']=[[True],[False]]
  self.assertNotEqual(self.early().returncode,0);self.assertNotEqual(self.eligibility().returncode,0)
  self.state['threads']=[[True],[True]]
  self.assertEqual(self.early().returncode,0);r=self.eligibility();self.assertEqual(r.returncode,0,r.stdout+r.stderr)
  self.state['thread_error']=True;r=self.early()
  self.assertEqual(r.returncode,0);self.assertIn('::warning::review-thread pre-check',r.stdout)
  self.assertNotEqual(self.eligibility().returncode,0)
 def test_final_live_head_entry_request_and_new_threads_still_refuse(self):
  for key,value in [('head','9'*40),('entry','MQE_NEW'),('attempt',2),('threads',[[True],[False]]),('pull_error',True)]:
   with self.subTest(key=key):
    before=copy.deepcopy(self.state);self.assertEqual(self.early().returncode,0)
    self.state[key]=value;self.assertNotEqual(self.eligibility().returncode,0);self.state=before

suite=unittest.defaultTestLoader.loadTestsFromTestCase(ReviewPreflightControls)
result=unittest.TextTestRunner(verbosity=2).run(suite)
sys.exit(not result.wasSuccessful())
"#;

#[test]
fn queue_review_preflight_and_terminal_currentness() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .ok_or("repository root missing")?;
    let output = Command::new("timeout")
        .args(["--kill-after=1s", "30s", "python3", "-I", "-c", CONTROLS])
        .arg(root.join(".github/actions/pr-candidate-admission/review_disposition.py"))
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
