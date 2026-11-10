//! Execute the trusted read-only Hermes intake consumer, including wrong implementations.
#![cfg(target_os = "linux")]

use std::process::Command;

const CONTROLS: &str = r#"
import copy, importlib.util, json, pathlib, subprocess, sys, tomllib, unittest
from datetime import datetime, timedelta, timezone
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
# An optional nested closure preserves recursive-tree coverage even when its
# containing tools tree is mandatory.
CLOSURE['tools/repo-policy']={'kind':'tree','mode':'040000','sha':'e'*40}
if 'tools' in CLOSURE:CLOSURE['tools']['sha']='8'*40
DATA={'schema_id':'hermes.queue-intake.v2','schema_version':2,'repository':m.REPOSITORY,
      'pull_number':42,'head_sha':H,'entry_id':'MQE_42','admission_request_id':'admit-1',
      'enqueue_request_id':'enqueue-1','base_sha':'f'*40,'expires_at':'2026-11-09T00:00:00Z',
      'authority_sha256':'1'*64,'producer_generation_sha256':'2'*64,'producer_closure':CLOSURE}
CHECK={'id':123,'app':{'id':9001},'name':'CI Lifecycle Qualification','head_sha':H,
       'external_id':'hermes-queue:admit-1:H','status':'completed','conclusion':'success','output':{
        'title':'H: conditional queue admission; G is not tested',
        'summary':m.canonical({'schema':'hermes.queue-check.v1','phase':'H','repository':m.REPOSITORY,
                              'commit':H,'evidence_sha256':'4'*64}),
        'text':m.canonical(DATA)}}
def withdraw(check):
 # Actual Hermes withdraw() PATCH, including replacement of the H summary.
 check.update(status='completed',conclusion='failure',output={
  'title':'Queue authority withdrawn',
  'summary':'A current blocking finding revoked conditional landing authority. Read native landing/removal state.'})
ROW={'id':'MQE_42','position':1,'baseCommit':{'oid':B},'headCommit':{'oid':G},
     'pullRequest':{'id':'PR_42','number':42,'state':'OPEN','isDraft':False,'headRefOid':H,
                    'baseRefName':'main','headRepository':{'nameWithOwner':m.REPOSITORY},'mergeQueueEntry':{'id':'MQE_42'}}}
QUEUE={'data':{'repository':{'id':'R_1','nameWithOwner':m.REPOSITORY,'mergeQueue':{'id':'MQ_1','entries':
       {'totalCount':1,'pageInfo':{'hasNextPage':False},'nodes':[ROW]}}}}}

class API:
 def __init__(self):
  self.checks={'total_count':1,'check_runs':[copy.deepcopy(CHECK)]}; self.q=copy.deepcopy(QUEUE)
  root=[{'path':k,'type':v['kind'],'mode':v['mode'],'sha':v['sha']} for k,v in CLOSURE.items() if '/' not in k and k!='tools']
  root.append({'path':'tools','type':'tree','mode':'040000','sha':'8'*40})
  root.append({'path':'editors','type':'tree','mode':'040000','sha':'7'*40})
  self.trees={T:{'sha':T,'truncated':False,'tree':root},'8'*40:{'sha':'8'*40,'truncated':False,'tree':[
   {'path':'repo-policy','type':'tree','mode':'040000','sha':'e'*40}]},'7'*40:{'sha':'7'*40,'truncated':False,'tree':[
   {'path':'vscode','type':'tree','mode':'040000','sha':'e'*40}]}}
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

class PublicationAPI(API):
 def __init__(self,publish_after=1):
  super().__init__();self.publish_after=publish_after;self.payload=copy.deepcopy(DATA)
  self.publication=self.checks['check_runs'][0];self.publication['output'].pop('text')
 def get(self,path):
  if '/check-runs?' in path and self.publish_after is not None and self.check_count>=self.publish_after:
   self.publication['output']['text']=m.canonical(self.payload)
  return super().get(path)

class Clock:
 def __init__(self,after_wait=None):
  self.elapsed=0;self.sleeps=[];self.after_wait=after_wait
 def read(self):return self.elapsed
 def wait(self,seconds):
  self.sleeps.append(seconds);self.elapsed+=seconds
  if self.after_wait:self.after_wait()

class Controls(unittest.TestCase):
 def refused(self,call):
  with self.assertRaises((m.Refused,ValueError,KeyError,TypeError)):call()
 def test_required_producer_roots_exist_in_the_committed_repository(self):
  root=p.parents[3]
  configured=tomllib.loads((root/'policy/queue-publisher.toml').read_text())
  self.assertEqual(configured['producer_closure'],m.ROOTS)
  for path,kind in m.ROOTS.items():
   with self.subTest(path=path):
    result=subprocess.run(['git','-C',str(root),'ls-tree','HEAD','--',path],
                          check=True,text=True,capture_output=True,timeout=5)
    rows=result.stdout.splitlines()
    self.assertEqual(len(rows),1,'required producer root must exist in the actual Git tree')
    metadata,actual_path=rows[0].split('\t')
    mode,actual_kind,sha=metadata.split()
    self.assertEqual(actual_path,path);self.assertEqual(actual_kind,kind)
    self.assertEqual(mode,'040000' if kind=='tree' else '100644')
    self.assertTrue(m.oid(sha))
 def test_exact_intake_allows_reviewed_new_generation_and_base_movement(self):
  api=API();r=m.observe(api,SUBJECT,m.configuration(CONFIG),lambda:NOW)
  self.assertEqual(r['member'],MEMBER);self.assertNotEqual(r['admission']['intake']['base_sha'],B)
  self.assertEqual(api.queue_count,2);self.assertEqual(api.check_count,2)
  self.assertEqual(m.observe(API(),SUBJECT,CONFIG,lambda:NOW,r),r)
 def test_intake_publication_can_follow_native_queue_event(self):
  # Hermes first publishes H success, enqueues, then PATCHes output.text with
  # the acknowledged entry. The initial H check is authentic but incomplete.
  api=PublicationAPI();clock=Clock()
  r=m.observe(api,SUBJECT,CONFIG,lambda:NOW,clock=clock.read,wait=clock.wait)
  self.assertEqual(r['member'],MEMBER)
  self.assertGreaterEqual(api.check_count,3)
  self.assertEqual(clock.sleeps,[m.INITIAL_INTAKE_INTERVAL])
 def test_pending_intake_ignores_conclusively_withdrawn_h_history(self):
  for position in [0,1]:
   for text in ['absent','null']:
    with self.subTest(position=position,text=text):
     api=PublicationAPI();old=copy.deepcopy(CHECK)
     old['id']=122;old['external_id']='hermes-queue:older:H';withdraw(old)
     if text=='null':old['output']['text']=None
     api.checks['check_runs'].insert(position,old);api.checks['total_count']=2
     clock=Clock();r=m.observe(api,SUBJECT,CONFIG,lambda:NOW,clock=clock.read,wait=clock.wait)
     self.assertEqual(r['admission']['check_run_id'],123)
     self.assertEqual(clock.sleeps,[m.INITIAL_INTAKE_INTERVAL]);self.assertEqual(api.check_count,3)
 def test_pending_history_must_prove_distinct_withdrawn_authority(self):
  for change in ['status','conclusion','title','summary','head','id','external','same_check','same_request']:
   with self.subTest(change=change):
    api=PublicationAPI();old=copy.deepcopy(CHECK)
    old['id']=122;old['external_id']='hermes-queue:older:H';withdraw(old)
    if change=='status':old['status']='in_progress'
    elif change=='conclusion':old['conclusion']='cancelled'
    elif change=='title':old['output']['title']='unknown failure'
    elif change=='summary':old['output']['summary']='unknown revocation'
    elif change=='head':old['head_sha']='9'*40
    elif change=='id':old['id']=True
    elif change=='external':old['external_id']='hermes-queue:older:G'
    elif change=='same_check':old['id']=123
    else:old['external_id']=CHECK['external_id']
    api.checks['check_runs'].append(old);api.checks['total_count']=2;clock=Clock()
    self.refused(lambda:m.observe(api,SUBJECT,CONFIG,lambda:NOW,clock=clock.read,wait=clock.wait))
    self.assertEqual(clock.sleeps,[]);self.assertEqual(api.check_count,1)
    self.assertFalse(any('/git/' in path for path in api.reads))
 def test_frozen_pending_withdrawal_never_becomes_ignorable_history(self):
  for replacement in [False,True]:
   with self.subTest(replacement=replacement):
    api=PublicationAPI(publish_after=None)
    def revoke():
     withdraw(api.publication)
     if replacement:
      other=copy.deepcopy(CHECK);other['id']=124;other['external_id']='hermes-queue:replacement:H'
      other['output'].pop('text');api.checks['check_runs'].append(other);api.checks['total_count']=2
    clock=Clock(revoke)
    self.refused(lambda:m.observe(api,SUBJECT,CONFIG,lambda:NOW,clock=clock.read,wait=clock.wait))
    self.assertEqual(len(clock.sleeps),1);self.assertEqual(api.check_count,2)
    self.assertFalse(any('/git/' in path for path in api.reads))
 def test_initial_wait_is_bounded_and_rechecks_native_membership(self):
  api=PublicationAPI(publish_after=2);clock=Clock()
  r=m.observe(api,SUBJECT,CONFIG,lambda:NOW,clock=clock.read,wait=clock.wait)
  self.assertEqual(r['member'],MEMBER)
  self.assertEqual(clock.sleeps,[m.INITIAL_INTAKE_INTERVAL]*2)
  self.assertEqual(api.queue_count,4);self.assertEqual(api.check_count,4)
  api=PublicationAPI(publish_after=None);clock=Clock()
  with self.assertRaisesRegex(m.Refused,'publication timed out'):
   m.observe(api,SUBJECT,CONFIG,lambda:NOW,clock=clock.read,wait=clock.wait)
  self.assertEqual(clock.elapsed,m.INITIAL_INTAKE_SECONDS)
  self.assertLessEqual(api.check_count,m.INITIAL_INTAKE_ATTEMPTS)
  self.assertTrue(all(0<s<=m.INITIAL_INTAKE_INTERVAL for s in clock.sleeps))
  self.assertFalse(any('/git/' in path for path in api.reads))
 def test_initial_wait_also_caps_observations_when_clock_does_not_advance(self):
  api=PublicationAPI(publish_after=None);sleeps=[]
  with self.assertRaisesRegex(m.Refused,'publication timed out'):
   m.observe(api,SUBJECT,CONFIG,lambda:NOW,clock=lambda:0,wait=sleeps.append)
  self.assertEqual(api.check_count,m.INITIAL_INTAKE_ATTEMPTS)
  self.assertEqual(len(sleeps),m.INITIAL_INTAKE_ATTEMPTS-1)
 def test_wait_never_adopts_a_changed_head_or_queue_entry(self):
  for change in ['head','entry']:
   with self.subTest(change=change):
    api=PublicationAPI()
    def moved():
     row=api.q['data']['repository']['mergeQueue']['entries']['nodes'][0]
     if change=='head':row['pullRequest']['headRefOid']='9'*40
     else:row['id']=row['pullRequest']['mergeQueueEntry']['id']='MQE_NEW'
    clock=Clock(moved)
    with self.assertRaisesRegex(m.Refused,'changed while awaiting publication'):
     m.observe(api,SUBJECT,CONFIG,lambda:NOW,clock=clock.read,wait=clock.wait)
    self.assertEqual(api.check_count,1);self.assertEqual(len(clock.sleeps),1)
 def test_pending_check_and_authority_are_frozen_until_publication(self):
  for change in ['check','request','summary']:
   for publish_after in [1,None]:
    with self.subTest(change=change,publish_after=publish_after):
     api=PublicationAPI(publish_after=publish_after)
     def moved():
      check=api.checks['check_runs'][0]
      if change=='check':check['id']=124
      elif change=='request':check['external_id']='hermes-queue:other:H'
      else:
       summary=json.loads(check['output']['summary']);summary['evidence_sha256']='5'*64
       check['output']['summary']=m.canonical(summary)
     clock=Clock(moved)
     self.refused(lambda:m.observe(api,SUBJECT,CONFIG,lambda:NOW,clock=clock.read,wait=clock.wait))
     self.assertEqual(len(clock.sleeps),1)
 def test_untrusted_revoked_and_malformed_pending_checks_never_wait(self):
  for change in ['app','name','head','external','status','revoked','id','title','summary',
                 'summary_head','summary_extra','summary_digest','text','empty','missing','ambiguous']:
   with self.subTest(change=change):
    api=PublicationAPI(publish_after=None);check=api.checks['check_runs'][0]
    if change=='app':check['app']['id']=9002
    elif change=='name':check['name']='lookalike'
    elif change=='head':check['head_sha']='9'*40
    elif change=='external':check['external_id']='hermes-queue:admit-1:G'
    elif change=='status':check['status']='in_progress'
    elif change=='revoked':check['conclusion']='cancelled'
    elif change=='id':check['id']=True
    elif change=='title':check['output']['title']='G is qualified'
    elif change=='summary':check['output']['summary']='not JSON'
    elif change.startswith('summary_'):
     summary=json.loads(check['output']['summary'])
     if change=='summary_head':summary['commit']='9'*40
     elif change=='summary_extra':summary['extra']='unrecognized'
     else:summary['evidence_sha256']='not-a-digest'
     check['output']['summary']=m.canonical(summary)
    elif change=='text':check['output']['text']=[]
    elif change=='empty':check['output']['text']=''
    elif change=='missing':api.checks={'total_count':0,'check_runs':[]}
    else:api.checks['check_runs']*=2;api.checks['total_count']=2
    clock=Clock()
    self.refused(lambda:m.observe(api,SUBJECT,CONFIG,lambda:NOW,clock=clock.read,wait=clock.wait))
    self.assertEqual(clock.sleeps,[])
    self.assertFalse(any('/git/' in path for path in api.reads))
 def test_revocation_during_initial_wait_refuses_without_another_wait(self):
  api=PublicationAPI(publish_after=None)
  def revoke():api.checks['check_runs'][0]['conclusion']='cancelled'
  clock=Clock(revoke)
  self.refused(lambda:m.observe(api,SUBJECT,CONFIG,lambda:NOW,clock=clock.read,wait=clock.wait))
  self.assertEqual(len(clock.sleeps),1);self.assertEqual(api.check_count,2)
 def test_receipt_expiring_during_wait_refuses_without_another_wait(self):
  api=PublicationAPI();clock=Clock()
  api.payload['expires_at']=(NOW+timedelta(seconds=1)).isoformat()
  with self.assertRaisesRegex(m.Refused,'intake expired'):
   m.observe(api,SUBJECT,CONFIG,lambda:NOW+timedelta(seconds=clock.elapsed),clock=clock.read,wait=clock.wait)
  self.assertEqual(len(clock.sleeps),1);self.assertEqual(api.check_count,2)
 def test_complete_receipt_ignores_an_older_unpublished_check(self):
  for withdrawn in [False,True]:
   with self.subTest(withdrawn=withdrawn):
    api=API();old=copy.deepcopy(CHECK)
    old['id']=122;old['external_id']='hermes-queue:older:H';old['conclusion']='cancelled'
    old['output'].pop('text')
    if withdrawn:withdraw(old)
    api.checks['check_runs'].insert(0,old);api.checks['total_count']=2
    clock=Clock();r=m.observe(api,SUBJECT,CONFIG,lambda:NOW,clock=clock.read,wait=clock.wait)
    self.assertEqual(r['admission']['check_run_id'],123);self.assertEqual(clock.sleeps,[])
 def test_final_expected_intake_never_waits_for_missing_publication(self):
  previous=m.observe(API(),SUBJECT,CONFIG,lambda:NOW)
  api=PublicationAPI();clock=Clock()
  self.refused(lambda:m.observe(api,SUBJECT,CONFIG,lambda:NOW,previous,clock=clock.read,wait=clock.wait))
  self.assertEqual(api.check_count,1);self.assertEqual(clock.sleeps,[])
 def test_after_source_recheck_never_waits_or_accepts_late_publication(self):
  class MissingAfterSource(API):
   def get(self,path):
    if '/check-runs?' in path and self.check_count==1:self.checks['check_runs'][0]['output'].pop('text')
    return super().get(path)
  api=MissingAfterSource();clock=Clock()
  self.refused(lambda:m.observe(api,SUBJECT,CONFIG,lambda:NOW,clock=clock.read,wait=clock.wait))
  self.assertEqual(api.check_count,2);self.assertEqual(clock.sleeps,[])
  api=PublicationAPI()
  class SlowPublicationClock(Clock):
   def read(self):
    if api.check_count>=2:self.elapsed=m.INITIAL_INTAKE_SECONDS
    return self.elapsed
  clock=SlowPublicationClock()
  with self.assertRaisesRegex(m.Refused,'publication timed out'):
   m.observe(api,SUBJECT,CONFIG,lambda:NOW,clock=clock.read,wait=clock.wait)
  self.assertFalse(any('/git/' in path for path in api.reads))
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
  for mode in ['object','mode','truncated','missing','traversal','missing_cargo','changed_cargo']:
   with self.subTest(mode=mode):
    api=API()
    if mode=='object':api.trees[T]['tree'][0]['sha']='9'*40
    if mode=='mode':api.trees[T]['tree'][0]['mode']='120000'
    if mode=='truncated':api.trees[T]['truncated']=True
    if mode=='missing':api.trees[T]['tree']=[]
    if mode=='missing_cargo':
     data=copy.deepcopy(DATA);del data['producer_closure']['.cargo'];api.checks['check_runs'][0]['output']['text']=m.canonical(data)
    if mode=='changed_cargo':
     next(entry for entry in api.trees[T]['tree'] if entry['path']=='.cargo')['sha']='9'*40
    if mode=='traversal':
     data=copy.deepcopy(DATA);data['producer_closure']['../outside']=data['producer_closure']['.github'];api.checks['check_runs'][0]['output']['text']=m.canonical(data)
    self.refused(lambda:m.observe(api,SUBJECT,CONFIG,lambda:NOW))
 def test_currentness_is_rechecked_and_never_adopts_new_authority(self):
  for mode in ['head','entry','authority','generation','revoked']:
   with self.subTest(mode=mode):
    api=API();api.change=mode;self.refused(lambda:m.observe(api,SUBJECT,CONFIG,lambda:NOW))
  old=m.observe(API(),SUBJECT,CONFIG,lambda:NOW);old['admission']['check_run_id']=999
  self.refused(lambda:m.observe(API(),SUBJECT,CONFIG,lambda:NOW,old))
 def test_command_and_policy_configuration_is_part_of_authorized_producer(self):
  for path in ['.cargo','clippy.toml','deny.toml']:
   for change in ['omitted','changed']:
    with self.subTest(path=path,change=change):
     api=API()
     if change=='omitted':
      data=copy.deepcopy(DATA);data['producer_closure'].pop(path,None);api.checks['check_runs'][0]['output']['text']=m.canonical(data)
     else:next(entry for entry in api.trees[T]['tree'] if entry['path']==path)['sha']='9'*40
     self.refused(lambda:m.observe(api,SUBJECT,CONFIG,lambda:NOW))
 def test_python_and_editor_proof_drivers_are_bound(self):
  for path,tree,name in [('tools',T,'tools'),('editors/vscode','7'*40,'vscode')]:
   with self.subTest(path=path):
    self.assertEqual(m.ROOTS.get(path),'tree','required proof drivers must be bound')
    api=API();data=copy.deepcopy(DATA);del data['producer_closure'][path]
    api.checks['check_runs'][0]['output']['text']=m.canonical(data)
    self.refused(lambda:m.observe(api,SUBJECT,CONFIG,lambda:NOW))
    api=API();next(entry for entry in api.trees[tree]['tree'] if entry['path']==name)['sha']='9'*40
    self.refused(lambda:m.observe(api,SUBJECT,CONFIG,lambda:NOW))
 def test_optional_nested_closure_is_checked(self):
  api=API();next(entry for entry in api.trees['8'*40]['tree'] if entry['path']=='repo-policy')['sha']='9'*40
  self.refused(lambda:m.observe(api,SUBJECT,CONFIG,lambda:NOW))
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
fn authenticated_intake_protocol_and_currentness_controls() -> Result<(), Box<dyn std::error::Error>>
{
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .ok_or("repository root missing")?;
    let output = Command::new("timeout")
        .args(["--kill-after=1s", "30s", "python3", "-I", "-c", CONTROLS])
        .arg(root.join(".github/actions/pr-candidate-admission/queue_intake.py"))
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
