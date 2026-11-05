import pathlib,json,subprocess,tempfile,shutil,hashlib,time,os,signal
r=pathlib.Path('/workspace/ripr-011-current-main');p=pathlib.Path('/workspace/011-integration-proof/private-field-prefill-admitted-measurement');p.mkdir(exist_ok=True);b=pathlib.Path('/workspace/12010-proof/target/debug/ripr');sha=hashlib.sha256(b.read_bytes()).hexdigest();version=subprocess.check_output([str(b),'--version'],text=True).strip();tmp=pathlib.Path('/dev/shm/ripr-011-qualification');tmp.mkdir(parents=True,exist_ok=True);rows=[]
def fixture(n):
 root=pathlib.Path(tempfile.mkdtemp(prefix='private-field-workload-',dir=tmp));(root/'src').mkdir();(root/'Cargo.toml').write_text('[package]\nname="private_field_workload"\nversion="0.0.0"\nedition="2024"\n[workspace]\n');(root/'src/lib.rs').write_text(''.join(f'mod m{i};\n' for i in range(n)))
 for i in range(n):
  t=f'Counter{i}';(root/f'src/m{i}.rs').write_text(f'pub struct {t} {{ count:u32 }}\nimpl {t} {{\n pub fn new()->Self {{ {t} {{count:0}} }}\n pub fn bump{i}(&mut self) {{\n  self.count=2;\n }}\n}}\n#[cfg(test)]\nmod tests {{\n use super::*;\n #[test]\n fn weak_read() {{\n  let mut c={t}::new();\n  c.bump{i}();\n  assert!(c.count>0);\n }}\n}}\n')
 return root
def run(n,label,root,cap):
 env=os.environ.copy();env['RIPR_REPO_EXPOSURE_SEAM_LIMIT']=str(cap);start=time.monotonic();peak=0;stop=None;cmd=[str(b),'check','--root',str(root),'--format','repo-exposure-json','--mode','draft'];out=p/f'{n}-{label}.json';err=p/f'{n}-{label}.stderr'
 with out.open('wb') as a,err.open('wb') as e:
  child=subprocess.Popen(cmd,cwd=r,env=env,stdout=a,stderr=e,start_new_session=True)
  while child.poll() is None:
   try:
    status=pathlib.Path(f'/proc/{child.pid}/status').read_text();rss=next((int(l.split()[1])*1024 for l in status.splitlines() if l.startswith('VmRSS:')),0);peak=max(peak,rss)
   except FileNotFoundError:pass
   if time.monotonic()-start>120:stop='120_second_budget'
   elif int(pathlib.Path('/sys/fs/cgroup/memory.current').read_text())>15.5*1024**3:stop='memory_budget'
   if stop:
    os.killpg(child.pid,signal.SIGTERM)
    try:child.wait(timeout=10)
    except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait()
    break
   time.sleep(.02)
  code=child.wait()
 d=json.loads(out.read_text()) if code==0 else {};row={'modules':n,'label':label,'cap':cap,'exit':code,'guard_stop':stop,'seconds':round(time.monotonic()-start,3),'peak_process_rss_bytes':peak,'top_level_keys':list(d),'summary':d.get('summary'),'cache':d.get('cache'),'limitations':d.get('limitations'),'seam_rows':len(d.get('seams',[])), 'private_write_rows':sum(1 for seam in d.get('seams',[]) if seam.get('expression','').startswith('self.count'))};rows.append(row);print(json.dumps(row),flush=True);(p/'receipt.json').write_text(json.dumps({'producer_version':version,'producer_sha256':sha,'rows':rows,'limitation':'Bounded synthetic multi-file private-field workload; no general performance claim; producer0402, dead-helper cleanup not runtime-qualified.'},indent=2)+'\n');return row
for n in [16,64]:
 full=fixture(n);bound=fixture(n)
 try:
  a=run(n,'cold-full',full,0)
  if a['exit'] or a['seconds']>60:break
  z=run(n,'cold-bounded',bound,1)
  if z['exit'] or z['seconds']>60:break
  run(n,'uncapped-after-bounded',bound,0)
 finally:shutil.rmtree(full);shutil.rmtree(bound)
assert hashlib.sha256(b.read_bytes()).hexdigest()==sha
