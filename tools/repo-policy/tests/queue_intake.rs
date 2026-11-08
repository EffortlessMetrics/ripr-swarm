//! Execute the trusted read-only Hermes intake consumer, including wrong implementations.
#![cfg(target_os = "linux")]

use std::process::Command;

const CONTROLS: &str = r#"
import copy, importlib.util, json, pathlib, sys, unittest
from datetime import datetime, timezone
p=pathlib.Path(sys.argv[1]); spec=importlib.util.spec_from_file_location('intake',p)
m=importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
B,H,G,T='a'*40,'b'*40,'c'*40,'d'*40
NOW=datetime(2026,11,8,tzinfo=timezone.utc)
CONFIG={'schema_version':1,'enabled':True,'repository':m.REPOSITORY,'base_branch':'main',
        'publisher_app_id':9001,'publisher_check_name':'CI Lifecycle Qualification','producer_closure':m.ROOTS}
SUBJECT={'repository':m.REPOSITORY,'repository_node_id':'R_1','base_sha':B,'group_sha':G,
         'head_ref':'refs/heads/gh-readonly-queue/main/pr-42-'+H,'request':'12-1'}
MEMBER={'pull_number':42,'pr_id':'PR_42','head_sha':H,'entry_id':'MQE_42'}
CLOSURE={k:{'kind':v,'mode':'040000' if v=='tree' else '100644','sha':'e'*40} for k,v in m.ROOTS.items()}
DATA={'schema_id':'hermes.queue-intake.v2','schema_version':2,'repository':m.REPOSITORY,
      'pull_number':42,'head_sha':H,'entry_id':'MQE_42','admission_request_id':'admit-1',
      'enqueue_request_id':'enqueue-1','base_sha':'f'*40,'expires_at':'2026-11-09T00:00:00Z',
      'authority_sha256':'1'*64,'producer_generation_sha256':'2'*64,'producer_closure':CLOSURE}
CHECK={'id':123,'app':{'id':9001},'name':'CI Lifecycle Qualification','head_sha':H,
       'external_id':'hermes-queue:admit-1:H','status':'completed','conclusion':'success','output':{'text':m.canonical(DATA)}}
ROW={'id':'MQE_42','position':1,'baseCommit':{'oid':B},'headCommit':{'oid':G},
     'pullRequest':{'id':'PR_42','number':42,'state':'OPEN','isDraft':False,'headRefOid':H,
                    'baseRefName':'main','headRepository':{'nameWithOwner':m.REPOSITORY},'mergeQueueEntry':{'id':'MQE_42'}}}
QUEUE={'data':{'repository':{'id':'R_1','nameWithOwner':m.REPOSITORY,'mergeQueue':{'id':'MQ_1','entries':
       {'totalCount':1,'pageInfo':{'hasNextPage':False},'nodes':[ROW]}}}}}

class API:
 def __init__(self):
  self.checks={'total_count':1,'check_runs':[copy.deepcopy(CHECK)]}; self.q=copy.deepcopy(QUEUE)
  root=[{'path':k,'type':v['kind'],'mode':v['mode'],'sha':v['sha']} for k,v in CLOSURE.items() if '/' not in k]
  root.append({'path':'tools','type':'tree','mode':'040000','sha':'8'*40})
  self.trees={T:{'sha':T,'truncated':False,'tree':root},'8'*40:{'sha':'8'*40,'truncated':False,'tree':[
   {'path':'repo-policy','type':'tree','mode':'040000','sha':'e'*40}]}}
  self.reads=[];self.queue_count=0;self.check_count=0;self.change=None
 def queue(self):
  self.queue_count+=1
  if self.change=='head' and self.queue_count==2:self.q['data']['repository']['mergeQueue']['entries']['nodes'][0]['pullRequest']['headRefOid']='9'*40
  if self.change=='entry' and self.queue_count==2:self.q['data']['repository']['mergeQueue']['entries']['nodes'][0]['id']='MQE_NEW'
  return copy.deepcopy(self.q)
 def get(self,path):
  self.reads.append(path)
  if '/check-runs?' in path:
   self.check_count+=1
   if self.check_count==2 and self.change in {'authority','generation','revoked'}:
    c=self.checks['check_runs'][0]
    if self.change=='revoked':c['conclusion']='cancelled'
    else:
     data=json.loads(c['output']['text']);data['authority_sha256' if self.change=='authority' else 'producer_generation_sha256']='3'*64;c['output']['text']=m.canonical(data)
   return copy.deepcopy(self.checks)
  if path=='/git/commits/'+G:return {'sha':G,'tree':{'sha':T}}
  if path.startswith('/git/trees/'):return copy.deepcopy(self.trees[path.rsplit('/',1)[1]])
  raise AssertionError(path)

class Controls(unittest.TestCase):
 def refused(self,call):
  with self.assertRaises((m.Refused,ValueError,KeyError,TypeError)):call()
 def test_exact_intake_allows_reviewed_new_generation_and_base_movement(self):
  api=API();r=m.observe(api,SUBJECT,m.configuration(CONFIG),lambda:NOW)
  self.assertEqual(r['member'],MEMBER);self.assertNotEqual(r['admission']['intake']['base_sha'],B)
  self.assertEqual(api.queue_count,2);self.assertEqual(api.check_count,2)
  self.assertEqual(m.observe(API(),SUBJECT,CONFIG,lambda:NOW,r),r)
 def test_binding_is_disabled_until_actual_app_is_configured(self):
  for key,value in [('enabled',False),('publisher_app_id',0),('publisher_app_id',True),('publisher_app_id',15368),('repository','attacker/repo')]:
   with self.subTest(key=key,value=value):
    c=copy.deepcopy(CONFIG);c[key]=value;self.refused(lambda:m.configuration(c))
 def test_wrong_check_authority_never_reaches_source_reads(self):
  for key,value in [('app',{'id':9002}),('name','lookalike'),('head_sha','9'*40),('external_id','hermes-queue:other:H'),('status','in_progress'),('conclusion','skipped'),('conclusion','neutral'),('conclusion','cancelled'),('id',True)]:
   with self.subTest(key=key,value=value):
    api=API();api.checks['check_runs'][0][key]=value
    self.refused(lambda:m.observe(api,SUBJECT,CONFIG,lambda:NOW))
    self.assertFalse(any('/git/' in x for x in api.reads))
 def test_receipt_fields_expiry_and_exact_head_are_discriminating(self):
  for key,value in [('schema_version',1),('schema_version',True),('schema_id','hermes.queue-intake.v1'),('head_sha','9'*40),('entry_id','MQE_OTHER'),('pull_number',43),('pull_number',True),('repository','attacker/repo'),('authority_sha256',''),('producer_generation_sha256',''),('admission_request_id','wrong\nvalue'),('expires_at','2026-11-07T00:00:00Z'),('expires_at','2026-11-09T00:00:00'),('producer_closure',{})]:
   with self.subTest(key=key,value=value):
    api=API();data=copy.deepcopy(DATA);data[key]=value;api.checks['check_runs'][0]['output']['text']=m.canonical(data)
    self.refused(lambda:m.observe(api,SUBJECT,CONFIG,lambda:NOW))
 def test_ambiguous_truncated_missing_and_duplicate_json_refuse(self):
  for mode in ['ambiguous','truncated','missing','duplicate','nonfinite','oversize']:
   with self.subTest(mode=mode):
    api=API()
    if mode=='ambiguous':api.checks['check_runs']*=2;api.checks['total_count']=2
    if mode=='truncated':api.checks['total_count']=101
    if mode=='missing':api.checks={'total_count':0,'check_runs':[]}
    if mode=='duplicate':api.checks['check_runs'][0]['output']['text']='{"entry_id":"MQE_42","entry_id":"MQE_42"}'
    if mode=='nonfinite':api.checks['check_runs'][0]['output']['text']='{"entry_id":"MQE_42","extra":NaN}'
    if mode=='oversize':api.checks['check_runs'][0]['output']['text']=' '* (m.MAX_BYTES+1)
    self.refused(lambda:m.observe(api,SUBJECT,CONFIG,lambda:NOW))
 def test_actual_producer_objects_modes_and_complete_roots(self):
  for mode in ['object','mode','truncated','missing','traversal']:
   with self.subTest(mode=mode):
    api=API()
    if mode=='object':api.trees[T]['tree'][0]['sha']='9'*40
    if mode=='mode':api.trees[T]['tree'][0]['mode']='120000'
    if mode=='truncated':api.trees[T]['truncated']=True
    if mode=='missing':api.trees[T]['tree']=[]
    if mode=='traversal':
     data=copy.deepcopy(DATA);data['producer_closure']['../outside']=data['producer_closure']['.github'];api.checks['check_runs'][0]['output']['text']=m.canonical(data)
    self.refused(lambda:m.observe(api,SUBJECT,CONFIG,lambda:NOW))
 def test_currentness_is_rechecked_and_never_adopts_new_authority(self):
  for mode in ['head','entry','authority','generation','revoked']:
   with self.subTest(mode=mode):
    api=API();api.change=mode;self.refused(lambda:m.observe(api,SUBJECT,CONFIG,lambda:NOW))
  old=m.observe(API(),SUBJECT,CONFIG,lambda:NOW);old['admission']['check_run_id']=999
  self.refused(lambda:m.observe(API(),SUBJECT,CONFIG,lambda:NOW,old))
 def test_native_join_rejects_incomplete_or_non_singleton_group(self):
  for mode in ['pagination','wrong_base','wrong_group','position','entry','draft']:
   with self.subTest(mode=mode):
    api=API();entries=api.q['data']['repository']['mergeQueue']['entries'];row=entries['nodes'][0]
    if mode=='pagination':entries['pageInfo']['hasNextPage']=True
    if mode=='wrong_base':row['baseCommit']['oid']='9'*40
    if mode=='wrong_group':row['headCommit']['oid']='9'*40
    if mode=='position':row['position']=2
    if mode=='entry':row['pullRequest']['mergeQueueEntry']['id']='other'
    if mode=='draft':row['pullRequest']['isDraft']=True
    self.refused(lambda:m.observe(api,SUBJECT,CONFIG,lambda:NOW))
 def test_event_identity_does_not_accept_pr_or_wrong_group(self):
  event={'action':'checks_requested','repository':{'full_name':m.REPOSITORY,'id':1243918801,'node_id':'R_1'},
         'merge_group':{'base_sha':B,'head_sha':G,'head_ref':SUBJECT['head_ref'],'base_ref':'refs/heads/main'}}
  env={'GITHUB_EVENT_NAME':'merge_group','GITHUB_REPOSITORY':m.REPOSITORY,'GITHUB_SHA':G,
       'GITHUB_REF':SUBJECT['head_ref'],'GITHUB_RUN_ID':'12','GITHUB_RUN_ATTEMPT':'1'}
  self.assertEqual(m.event_subject(event,env),SUBJECT)
  for key,value in [('GITHUB_EVENT_NAME','pull_request'),('GITHUB_SHA',H),('GITHUB_RUN_ATTEMPT','0')]:
   wrong=dict(env);wrong[key]=value;self.refused(lambda:m.event_subject(event,wrong))

suite=unittest.defaultTestLoader.loadTestsFromTestCase(Controls)
result=unittest.TextTestRunner(verbosity=2).run(suite)
sys.exit(not result.wasSuccessful())
"#;

#[test]
fn authenticated_intake_protocol_and_currentness_controls() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("repository root");
    let output = Command::new("timeout")
        .args(["--kill-after=1s", "30s", "python3", "-I", "-c", CONTROLS])
        .arg(root.join(".github/actions/pr-candidate-admission/queue_intake.py"))
        .output()
        .expect("bounded Python control launch");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
