import pathlib,json,hashlib,tarfile,shutil,os,sys
P=pathlib.Path('/workspace/scratch/8265-reconstruction');label=sys.argv[1];rows=[]
for l in (P/(label+'.stdout')).read_text().splitlines():
 try:rows.append(json.loads(l))
 except ValueError:pass
artifact=[x for x in rows if x.get('reason')=='compiler-artifact' and x.get('executable') and x['target']['name']=='ripr' and x['profile']['test']][-1];src=pathlib.Path(artifact['executable']);sha=lambda p:hashlib.file_digest(p.open('rb'),'sha256').hexdigest();h=sha(src);a=json.loads((P/'admission.json').read_text());assert shutil.disk_usage(P).free-src.stat().st_size-128*1024**2>max(a['reserve'],a['initial_free']-a['peak_additional']);arc=P/(label+'-tests.tar.gz');assert not arc.exists()
with tarfile.open(arc,'w:gz',compresslevel=6) as tf:tf.add(src,arcname='ripr-tests',recursive=False)
dst=P/(label+'-restore');assert not dst.exists()
with tarfile.open(arc,'r:gz') as tf:
 assert tf.getnames()==['ripr-tests'];m=tf.getmember('ripr-tests');assert m.isfile()
 with tf.extractfile(m) as inp,dst.open('xb') as out:shutil.copyfileobj(inp,out)
 os.chmod(dst,m.mode)
assert sha(dst)==h and dst.stat().st_size==src.stat().st_size and (dst.stat().st_mode&0o777)==(src.stat().st_mode&0o777)
r={'source_receipt':json.loads((P/(label+'.json')).read_text()),'artifact':artifact,'binary_sha256':h,'binary_bytes':src.stat().st_size,'archive_sha256':sha(arc),'archive_bytes':arc.stat().st_size,'full_restore_verified':True,'restored_duplicate_disposition':'removed only after archive full-byte/size/mode equality; archive retained'};(P/(label+'-binary.json')).write_text(json.dumps(r,indent=2)+'\n');dst.unlink();print(json.dumps({k:v for k,v in r.items() if k not in ['source_receipt','artifact']},indent=2))
