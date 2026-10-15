import pathlib,os,stat,hashlib,tarfile,json,shutil,time
root=pathlib.Path('/workspace/8004-repair');q=root/'verified-quarantine';q.mkdir(exist_ok=False);stage=q/'restore-verification';stage.mkdir()
paths=[]
for name in ['native-matrix-initial','native-matrix','native-additional']:
 paths += [p for p in (root/name).rglob('*') if p.is_file() and p.name in ['original','mutant']]
paths += [p for p in (root/'native-reviewer').iterdir() if p.is_file() and p.name.endswith(('-original','-mutant'))]
paths += [root/name for name in ['a2-ripr','green-ripr','fixed-ripr']]
assert len(paths)==201,len(paths)
def digest(p):
 h=hashlib.sha256()
 with p.open('rb') as f:
  for b in iter(lambda:f.read(1024*1024),b''):h.update(b)
 return h.hexdigest()
entries=[]
for p in sorted(paths):
 s=p.stat();assert stat.S_ISREG(s.st_mode) and not p.is_symlink();entries.append(dict(path=str(p.relative_to(root)),size=s.st_size,sha256=digest(p),mode=stat.S_IMODE(s.st_mode),mtime_ns=s.st_mtime_ns,uid=s.st_uid,gid=s.st_gid,inode=s.st_ino))
total=sum(x['size'] for x in entries);free=shutil.disk_usage(root).free
# Admit worst-case uncompressed archive plus complete restore while originals remain.
assert free > 8*1024**3 + total*2 + 64*1024**2,(free,total)
manifest=dict(owner='sole #8004 source repair',subject='Completed native witnesses and prior a2/ineffective/dirty-correct CLI binaries; current committed candidate excluded',completed_consumers=['baseline driver 90546 terminal 0','a2 CLI driver 87936 terminal 1 (behavioral red)','supported red/native driver 29639 terminal 0; its CLI subcommand exit 1 retained','native driver 2041 terminal 0','ineffective guard CLI driver 42358 terminal 1','dirty-correct CLI driver 19477 terminal 0','a2 --version inspection terminal 0'],reserve_bytes=8*1024**3,free_bytes_before=free,total_source_bytes=total,entries=entries)
(q/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
archive=q/'witnesses.tar.gz'
with tarfile.open(archive,'w:gz',compresslevel=1) as tar:
 for e in entries:tar.add(root/e['path'],arcname=e['path'],recursive=False)
print('archive created',archive.stat().st_size,flush=True)
with tarfile.open(archive,'r:gz') as tar:
 members={m.name:m for m in tar.getmembers()};assert set(members)=={e['path'] for e in entries}
 for e in entries:
  m=members[e['path']];assert m.isfile() and m.name==e['path'] and not pathlib.PurePosixPath(m.name).is_absolute() and '..' not in pathlib.PurePosixPath(m.name).parts
  p=stage/e['path'];p.parent.mkdir(parents=True,exist_ok=True)
  with tar.extractfile(m) as src,p.open('xb') as dst:shutil.copyfileobj(src,dst)
  os.chmod(p,e['mode']);os.utime(p,ns=(e['mtime_ns'],e['mtime_ns']))
  s=p.stat();assert s.st_size==e['size'] and digest(p)==e['sha256'] and stat.S_IMODE(s.st_mode)==e['mode'] and s.st_mtime_ns==e['mtime_ns'] and s.st_uid==e['uid'] and s.st_gid==e['gid'],e['path']
print('complete restored bytes and required metadata verified',len(entries),flush=True)
# Recheck exact originals after complete verification; no retirement on any mismatch.
for e in entries:
 p=root/e['path'];s=p.stat();assert s.st_ino==e['inode'] and s.st_size==e['size'] and s.st_mtime_ns==e['mtime_ns'] and stat.S_IMODE(s.st_mode)==e['mode'] and digest(p)==e['sha256'],e['path']
for e in entries:(root/e['path']).unlink()
# Staging contains only the restore copy created and verified by this driver.
shutil.rmtree(stage)
result=dict(status='verified recoverable retirement',archive=str(archive),archive_sha256=digest(archive),archive_bytes=archive.stat().st_size,manifest_sha256=digest(q/'manifest.json'),restored_files=len(entries),source_bytes=total,free_bytes_after=shutil.disk_usage(root).free,current_candidate_retained=str(root/'candidate-ripr'),raw_sources_and_logs_retained=True,required_metadata=['size','sha256','mode','mtime_ns','uid','gid'],restore='Read manifest.json; extract witnesses.tar.gz into a separate staging directory, verify each entry, restore required mode/mtime and verify uid/gid, then restore only missing original paths under /workspace/8004-repair. Archive retained; no permanent purge authorized.')
(q/'verification.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)
