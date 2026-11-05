import hashlib, json, os, re, shutil, signal, subprocess, tempfile, time
from pathlib import Path
proof=Path('/workspace/011-integration-proof');repo=Path('/workspace/ripr-011-current-main');head='e133ffc21a8acdd3eed6734373465ab83b03c226'
summary=json.loads((proof/'e133-golden-drift-summary.json').read_text());binary=Path(summary['frozen_producer']['path']);out=proof/'golden-native-pairs';out.mkdir(exist_ok=True)
assert hashlib.sha256(binary.read_bytes()).hexdigest()==summary['frozen_producer']['sha256']
temp=Path('/dev/shm/ripr-011-qualification');temp.mkdir(exist_ok=True);cargo=Path('/workspace/.rustup/toolchains/1.95.0-x86_64-unknown-linux-gnu/bin/cargo');env=os.environ.copy()
env.update(PATH=str(cargo.parent)+':/workspace/.cargo/bin:'+env['PATH'],CARGO_HOME='/workspace/.cargo',RUSTUP_HOME='/workspace/.rustup',RUSTUP_TOOLCHAIN='1.95.0',CARGO_PROFILE_TEST_DEBUG='0',CARGO_PROFILE_DEV_DEBUG='0',CARGO_BUILD_JOBS='1',CARGO_INCREMENTAL='0',TMPDIR=str(temp),TMP=str(temp),TEMP=str(temp))
for key in ['GIT_DIR','GIT_WORK_TREE','GIT_INDEX_FILE','GIT_COMMON_DIR']:env.pop(key,None)
def git(*args):return subprocess.check_output(['git',*args],cwd=repo)
def execute(command,cwd,stem,budget=120):
 start=time.monotonic();stdout=stem.with_suffix('.stdout');stderr=stem.with_suffix('.stderr');stop=None
 with stdout.open('wb') as a,stderr.open('wb') as b:
  child=subprocess.Popen(command,cwd=cwd,env=env,stdout=a,stderr=b,start_new_session=True)
  while child.poll() is None:
   memory=int(Path('/sys/fs/cgroup/memory.current').read_text());free=shutil.disk_usage('/workspace').free
   if time.monotonic()-start>budget:stop='time_budget'
   elif memory>15.5*1024**3:stop='memory_budget'
   elif free<4*1024**3:stop='disk_reserve'
   if stop:
    os.killpg(child.pid,signal.SIGTERM)
    try:child.wait(timeout=10)
    except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait()
    break
   time.sleep(.2)
 result={'command':list(map(str,command)),'exit':child.wait(),'guard_stop':stop,'seconds':round(time.monotonic()-start,3),'stdout_sha256':hashlib.sha256(stdout.read_bytes()).hexdigest(),'stderr_sha256':hashlib.sha256(stderr.read_bytes()).hexdigest()}
 text=stdout.read_text(errors='replace');result['tests']=[{'name':m[0],'status':m[1]} for m in re.findall(r'^test (.+?) \.\.\. (ok|FAILED|ignored)$',text,re.M)];result['nonempty_test_execution']=bool(result['tests']);return result
rows=[]
for name in summary['classification_changed']:
 case=out/name;case.mkdir(exist_ok=True);owned=Path(tempfile.mkdtemp(prefix='golden-native-',dir=temp));root=owned/'input';root.mkdir()
 prefix=f'fixtures/{name}/input/';files=git('ls-tree','-r','--name-only',head,'--',prefix).decode().splitlines()
 for path in files:
  rel=Path(path).relative_to(prefix);dest=root/rel;dest.parent.mkdir(parents=True,exist_ok=True);data=git('show',head+':'+path);dest.write_bytes(data);saved=case/'input'/rel;saved.parent.mkdir(parents=True,exist_ok=True);saved.write_bytes(data)
 patch=case/'diff.patch';patch.write_bytes(git('show',head+f':fixtures/{name}/diff.patch'))
 row={'fixture':name,'source_candidate':head,'source_files':files}
 row['producer']=execute([str(binary),'check','--root',str(root),'--diff',str(patch),'--mode','fast','--format','json'],repo,case/'producer')
 if row['producer']['exit']==0:
  data=json.loads((case/'producer.stdout').read_text());row['findings']=[{'family':f.get('probe',{}).get('family'), 'class':f.get('classification'),'owner':f.get('probe',{}).get('owner'),'line':f.get('probe',{}).get('line')} for f in data['findings']]
 command=[str(cargo),'test','--offline','--manifest-path',str(root/'Cargo.toml'),'--target-dir',str(out/'target'),'--','--test-threads=1']
 row['input']=execute(command,root,case/'input-native')
 row['reverse_patch']=execute(['git','apply','--reverse','--whitespace=nowarn',str(patch)],root,case/'reverse-patch')
 if row['reverse_patch']['exit']==0:
  row['before']=execute(command,root,case/'before-native')
  original={t['name']:t['status'] for t in row['input']['tests']};before={t['name']:t['status'] for t in row['before']['tests']};row['changed_native_test_outcomes']=[{'name':n,'input':original[n],'before':before[n]} for n in original.keys()&before.keys() if original[n]!=before[n]]
  for path in files:
   rel=Path(path).relative_to(prefix);source=root/rel
   if source.exists():dest=case/'before'/rel;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(source.read_bytes())
 shutil.rmtree(owned);rows.append(row);(out/'receipt.json').write_text(json.dumps({'source_candidate':head,'producer':summary['frozen_producer'],'rows':rows,'limitation':'Native paired outcomes diagnose golden drift; compile failures and zero-test executions do not establish observation. Complete-project failure may include deliberately wrong sibling controls; named test changes are retained.'},indent=2)+'\n')
 print(json.dumps({'fixture':name,'producer_exit':row['producer']['exit'],'input_exit':row['input']['exit'],'before_exit':row.get('before',{}).get('exit'),'nonempty':row['input']['nonempty_test_execution'],'changed':row.get('changed_native_test_outcomes',[]),'reverse_patch_exit':row['reverse_patch']['exit']}),flush=True)
assert hashlib.sha256(binary.read_bytes()).hexdigest()==summary['frozen_producer']['sha256']
print('GOLDEN_NATIVE_PAIRS_TERMINAL',flush=True)
