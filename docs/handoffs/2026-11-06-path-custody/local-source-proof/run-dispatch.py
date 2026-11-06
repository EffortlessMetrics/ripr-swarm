import os,sys,time,json,signal,subprocess,shutil,resource,hashlib
from pathlib import Path
root=Path('/workspace/ripr-path-combined'); out=Path('/workspace/ripr-docs-evidence/local-source-proof/dispatch'); target=Path('/workspace/ripr-path-native-target')
subject=subprocess.check_output(['git','rev-parse','HEAD','HEAD^{tree}'],cwd=root,text=True).splitlines()
assert subject==['e160bfbdc50579ee6e9fd82a78b41df55f3fa58c','8ae126c82973c71ef5cd3d9ef55216bc4b2c78ff']
assert not subprocess.check_output(['git','status','--porcelain'],cwd=root)
GiB=1024**3
limits={'disk_reserve_bytes':6*GiB,'peak_additional_bytes':18*GiB,'owned_rss_bytes':10*GiB,'per_process_address_space_bytes':12*GiB,'wall_seconds':2700,'jobs':1}
free=shutil.disk_usage('/workspace').free
assert free>limits['disk_reserve_bytes']+limits['peak_additional_bytes'],free
current=int(Path('/sys/fs/cgroup/memory.current').read_text()); cap=int(Path('/sys/fs/cgroup/memory.max').read_text())
assert cap-current>12*GiB,(cap,current)
args=['/workspace/ripr-cargo/bin/cargo','test','--locked','-p','xtask','--bin','xtask','reports::release::candidate_harness::live_head::producer::tests::','--','--nocapture']
env=os.environ.copy();env.update(RUSTUP_HOME='/workspace/ripr-rustup',CARGO_HOME='/workspace/ripr-cargo',CARGO_TARGET_DIR=str(target),CARGO_BUILD_JOBS='1',CARGO_INCREMENTAL='0',CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0')
admission={'subject_sha':subject[0],'subject_tree':subject[1],'cwd':str(root),'git_common_dir':str(root/'.git'),'target':str(target),'tmp':str(root/'target'),'retained_proof':str(out),'owner':'delegated docs/path lane','command':args,'environment':{k:env[k] for k in ['RUSTUP_HOME','CARGO_HOME','CARGO_TARGET_DIR','CARGO_BUILD_JOBS','CARGO_INCREMENTAL','CARGO_PROFILE_DEV_DEBUG','CARGO_PROFILE_TEST_DEBUG']},'limits':limits,'observed_free_bytes':free,'cgroup_memory_max':cap,'cgroup_memory_current':current,'start_utc':time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),'scope':'Actual xtask dispatch fixture group only; warm target; disposable local fixtures; no GitHub transport or activation'}
(out/'admission.json').write_text(json.dumps(admission,indent=2)+'\n')
start=time.monotonic(); maxrss=0; minfree=free;reason=None

def setup(): resource.setrlimit(resource.RLIMIT_AS,(limits['per_process_address_space_bytes'],limits['per_process_address_space_bytes']))
with (out/'cargo-native.log').open('wb') as log,(out/'guard-samples.jsonl').open('w') as samples:
 p=subprocess.Popen(args,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True,preexec_fn=setup)
 print('Native guarded run started; pid',p.pid,'subject',subject[0],flush=True)
 while p.poll() is None:
  rss=0; members=[]
  for node in Path('/proc').iterdir():
   if not node.name.isdigit():continue
   try:
    stat=(node/'stat').read_text(); vals=stat[stat.rfind(')')+2:].split()
    if int(vals[2])!=p.pid:continue
    page=int(vals[21]);rss+=page*os.sysconf('SC_PAGE_SIZE'); members.append(int(node.name))
   except (OSError,ValueError,IndexError):pass
  available=shutil.disk_usage('/workspace').free; elapsed=time.monotonic()-start
  maxrss=max(maxrss,rss);minfree=min(minfree,available)
  samples.write(json.dumps({'elapsed_seconds':round(elapsed,2),'owned_rss_bytes':rss,'free_bytes':available,'owned_pids':members})+'\n');samples.flush()
  if available<limits['disk_reserve_bytes']:reason='disk_reserve_reached'
  elif free-available>limits['peak_additional_bytes']:reason='peak_growth_budget_reached'
  elif rss>limits['owned_rss_bytes']:reason='owned_rss_budget_reached'
  elif elapsed>limits['wall_seconds']:reason='wall_budget_reached'
  if reason:
   os.killpg(p.pid,signal.SIGTERM)
   try:p.wait(timeout=5)
   except subprocess.TimeoutExpired:os.killpg(p.pid,signal.SIGKILL);p.wait()
   break
  time.sleep(2)
 code=p.wait()
result={'native_exit_code':code,'guard_stop_reason':reason,'elapsed_seconds':round(time.monotonic()-start,2),'peak_owned_rss_bytes':maxrss,'minimum_free_bytes':minfree,'subject_sha':subject[0],'subject_tree':subject[1],'raw_log_sha256':hashlib.sha256((out/'cargo-native.log').read_bytes()).hexdigest(),'end_utc':time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime())}
(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)
sys.exit(0 if code==0 and not reason else 1)
