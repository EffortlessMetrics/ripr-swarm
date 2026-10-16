# Review experiment driver

```python
import os,sys,pathlib,subprocess,json,hashlib,time,resource,signal,statistics
E=pathlib.Path('/workspace/8201-evidence/independent3'); E.mkdir(); (E/'raw').mkdir(); B=pathlib.Path(sys.argv[1]); BASE=E/'corpus'; BASE.mkdir(exist_ok=True)
env=os.environ.copy();env.update(CARGO_HOME='/workspace/.cargo',RUSTUP_HOME='/workspace/.rustup',PATH='/workspace/.cargo/bin:'+env['PATH'],RAYON_NUM_THREADS='2')
records=[]
def sha(b):return hashlib.sha256(b).hexdigest()
def write(p,t):p.parent.mkdir(parents=True,exist_ok=True);p.write_text(t)
def fixture(label,n=1,python=False):
 r=BASE/label;r.mkdir();write(r/'Cargo.toml','[package]\nname="independent"\nversion="0.0.0"\nedition="2024"\n[workspace]\n')
 write(r/'ripr.toml','[analysis]\nmode="ready"\n[languages]\nenabled=["rust","python"]\n[severity.findings]\nno_static_path="info"\n')
 lib='';diff=''
 for i in range(n):
  lib+=f'pub mod m{i};\n';source=f'pub fn positive(v: i32) -> bool {{ v > {i} }}\n';write(r/f'src/m{i}.rs',source)
  diff+=f'--- a/src/m{i}.rs\n+++ b/src/m{i}.rs\n@@ -1 +1 @@\n-'+source.replace(' > ',' >= ')+'+'+source
 write(r/'src/lib.rs',lib);write(r/'test_note.py','def test_unlinked():\n    assert True\n')
 if python:
  write(r/'src/price.py','def price(n):\n    if n >= 10:\n        return n - 2\n    return n\n')
  write(r/'tests/test_price.py','from src.price import price\ndef test_price():\n    assert price(10) == 8\n')
  diff='--- a/src/price.py\n+++ b/src/price.py\n@@ -1,4 +1,4 @@\n def price(n):\n-    if n > 10:\n+    if n >= 10:\n         return n - 2\n     return n\n'
 write(r/'input.diff',diff);return r

def run(r,label,args,extra=None,cap=None,mutate=None):
 e=env.copy();e.update(extra or {});cmd=[str(B),'check','--root',str(r),'--diff',str(r/'input.diff')]+args
 if not mutate:cmd+=['--quiet']
 def pre():
  signal.signal(signal.SIGXFSZ,signal.SIG_IGN);resource.setrlimit(resource.RLIMIT_FSIZE,(cap,cap))
 before=resource.getrusage(resource.RUSAGE_CHILDREN);t=time.perf_counter()
 if mutate:
  p=subprocess.Popen(cmd,env=e,stdout=subprocess.PIPE,stderr=subprocess.PIPE); lines=[];trigger=False
  for line in iter(p.stderr.readline,b''):
   lines.append(line)
   if b'ripr progress: analyzing ' in line and not trigger:mutate();trigger=True
  out=p.stdout.read();p.wait(timeout=60);err=b''.join(lines);code=p.returncode
  assert trigger,'mutation stimulus never reached'
 else:
  p=subprocess.run(cmd,env=e,capture_output=True,timeout=60,preexec_fn=pre if cap else None);out,err,code=p.stdout,p.stderr,p.returncode
 elapsed=time.perf_counter()-t;after=resource.getrusage(resource.RUSAGE_CHILDREN)
 write(E/'raw'/f'{label}.stdout',out.decode());write(E/'raw'/f'{label}.stderr',err.decode())
 rec=dict(label=label,args=cmd,exit=code,wall=elapsed,cpu=after.ru_utime+after.ru_stime-before.ru_utime-before.ru_stime,stdout_bytes=len(out),stderr_bytes=len(err));records.append(rec)
 return code,out,err,rec

def manifest(d):
 m=json.loads((d/'manifest.json').read_bytes())
 assert [x['file'] for x in m['members']]==['badge.json','detail.json','human.txt']
 for x in m['members']:
  b=(d/x['file']).read_bytes();assert len(b)==x['bytes'] and sha(b)==x['sha256']
 return m

def parity(r,label,extra=None,args=None,expected=0):
 args=args or [];d=E/label
 code,out,err,rec=run(r,label,['--output-dir',str(d)]+args,extra);assert code==expected,(code,err);assert not out
 m=manifest(d)
 for fmt,file in [('badge-json','badge.json'),('json','detail.json'),('human','human.txt')]:
  c,b,er,_=run(r,label+'-'+fmt,['--format',fmt]+args,extra);assert c==code,(fmt,c,code,er);assert b==(d/file).read_bytes(),label+'/'+fmt+' parity'
 return m

try:
 r=fixture('policy',8);write(r/'selected.toml','schema_version=1\n[[suppressions]]\nkind="exposure_gap"\npath="src/**"\nowner="review"\nreason="independent suppression"\nexpires="2099-01-01"\n')
 with (r/'ripr.toml').open('a') as f:f.write('[suppressions]\npath="selected.toml"\n')
 m=parity(r,'policy-effective',args=['--mode','deep']);assert m['complete_set'] and m['command_success'];assert 'Deep' in m['identity']['effective_input']
 write(r/'selected.toml',(r/'selected.toml').read_text().replace('2099-01-01','2000-01-01'));parity(r,'policy-expired')
 py=fixture('python',python=True);parity(py,'python-native');detail=json.loads((E/'python-native/detail.json').read_bytes());assert detail['findings'];assert detail['canonical_next_action'];stamps=detail['source_subject']['files'];assert len(stamps)>=1
 for s in stamps:assert s['digest']=='sha256:'+sha((py/s['path']).read_bytes())
 weak=fixture('python-weak',python=True);write(weak/'tests/test_price.py','from src.price import price\ndef test_price():\n    result = price(100)\n    if result is None: raise AssertionError(\"missing\")\n');parity(weak,'python-weak-native');assert len(json.loads((E/'python-weak-native/detail.json').read_bytes())['source_subject']['files'])>=2
 # Simulate a real kernel ENOSPC-style short regular-file write without replacing a helper.
 code,out,err,_=run(r,'partial-write',['--output-dir',str(E/'partial-write')],cap=1024);assert code==2 and b'incomplete' in err;assert not (E/'partial-write/manifest.json').exists();assert any(p.stat().st_size for p in (E/'partial-write').iterdir())
 # A malformed environment is a refusal, while unsupported analysis is a published failed command.
 write(py/'input.diff','--- a/file.go\n+++ b/file.go\n@@ -0,0 +1 @@\n+package main\n');m=parity(py,'unsupported');assert m['complete_set'] and m['command_success'];assert not json.loads((E/'unsupported/detail.json').read_bytes())['analysis_outcome']['analysis_complete']
 write(py/'input.diff','garbage');m=parity(py,'malformed',expected=2);assert m['complete_set'] and not m['command_success']
 write(py/'ripr.toml','[broken');m=parity(py,'config-refusal',expected=2);assert not m['complete_set'] and not m['command_success']
 # Persist changes only after actual loading has crossed the native analyzing event.
 for what in ['source','diff','config','suppression','membership']:
  dr=fixture('drift-'+what,16)
  targets={'source':dr/'src/m0.rs','diff':dr/'input.diff','config':dr/'ripr.toml','suppression':dr/'.ripr/suppressions.toml','membership':dr/'new_test.py'}
  def mutation(target=targets[what]):write(target,'changed persistently\n')
  dest=E/('drift-'+what);c,o,er,_=run(dr,'drift-'+what,['--output-dir',str(dest)],mutate=mutation);assert c==2 and b'drift' in er,(what,c,er)
  if (dest/'manifest.json').exists():assert not manifest(dest)['complete_set']
 # Destination parent symlink retarget, outside captured source root.
 dr=fixture('destination',16);p1=E/'parent1';p2=E/'parent2';p1.mkdir();p2.mkdir();link=E/'selected-parent';link.symlink_to(p1,target_is_directory=True)
 def retarget():link.unlink();link.symlink_to(p2,target_is_directory=True)
 c,o,er,_=run(dr,'destination-retarget',['--output-dir',str(link/'result')],mutate=retarget);assert c==2 and b'destination parent changed' in er
 for dest in [p1/'result',p2/'result']:
  if (dest/'manifest.json').exists():assert not manifest(dest)['complete_set']
 # Guard exclusion on a real reached Rust include; standalone remains supported.
 ex=fixture('exclusion');write(ex/'.git/private.rs','pub mod m0;\n');write(ex/'src/lib.rs','include!("../.git/private.rs");\n');c,o,er,_=run(ex,'excluded-read',['--output-dir',str(E/'excluded-read')]);assert c==2 and b'outside the root' in er
 c,o,er,_=run(ex,'excluded-standalone',['--format','json']);assert c==0,er
 # Bounded independent timings: 3 alternating pairs, 8 modules, warm analyzer state, no builds.
 tr=fixture('timing',8);run(tr,'timing-warmup',['--format','human']);pairs=[]
 for i in range(3):
  times={}
  for variant in (['separate','fanout'] if i%2==0 else ['fanout','separate']):
   if variant=='fanout':
    c,o,er,rr=run(tr,f'time-{i}-fanout',['--output-dir',str(E/f'time-{i}')]);assert c==0,er;manifest(E/f'time-{i}');times[variant]=(rr['wall'],rr['cpu'])
   else:
    rr=[]
    for fmt in ['badge-json','json','human']:
     c,o,er,re=run(tr,f'time-{i}-{fmt}',['--format',fmt]);assert c==0,er;rr.append(re)
    times[variant]=(sum(x['wall'] for x in rr),sum(x['cpu'] for x in rr))
  for fmt,file in [('badge-json','badge.json'),('json','detail.json'),('human','human.txt')]:assert (E/'raw'/f'time-{i}-{fmt}.stdout').read_bytes()==(E/f'time-{i}'/file).read_bytes()
  pairs.append(times)
 (E/'timings.json').write_text(json.dumps(pairs,indent=2))
 status='passed'
except BaseException as exc:
 status='failed: '+repr(exc);raise
finally:
 (E/'independent.json').write_text(json.dumps(dict(status=status,binary=str(B),binary_sha256=sha(B.read_bytes()),records=records),indent=2))
 print(status,flush=True)

```
