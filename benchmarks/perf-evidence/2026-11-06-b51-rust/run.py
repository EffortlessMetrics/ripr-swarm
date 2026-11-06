import os,sys,json,time,pathlib,subprocess,signal,hashlib,shutil,resource
out=pathlib.Path('/workspace/ripr-perf-evidence'); name=sys.argv[1]; cache=sys.argv[2]
binary=os.environ.get('RIPR_PERF_BINARY','/workspace/ripr-perf-target/release/ripr'); cwd=os.environ.get('RIPR_PERF_CWD','/workspace/ripr-perf-corpus')
diff=pathlib.Path(os.environ.get('RIPR_PERF_DIFF',str(out/'product1.diff')))
args=[binary,'check','--diff',str(diff),'--mode','draft','--format','json']
input_diff_sha256=hashlib.sha256(diff.read_bytes()).hexdigest()
env=os.environ.copy(); env.update(PATH='/workspace/.cargo/bin:'+env['PATH'],CARGO_HOME='/workspace/.cargo',RUSTUP_HOME='/workspace/.rustup',RIPR_CACHE_DIR=cache,RAYON_NUM_THREADS='4',RIPR_REPO_EXPOSURE_LATENCY_TRACE='1')
if len(sys.argv)>3: env['RIPR_PROBE_READ_TRACE']='1'
token='perf'+name.replace('-','');env['RIPR_REPO_EXPOSURE_LATENCY_RECEIPT_TOKEN']=token
start=time.monotonic(); status='running'
with (out/(name+'.json')).open('wb') as stdout,(out/(name+'.stderr')).open('wb') as stderr:
 p=subprocess.Popen(args,cwd=cwd,env=env,stdout=stdout,stderr=stderr,start_new_session=True)
 peak=0
 while p.poll() is None:
  rss=0
  for d in pathlib.Path('/proc').iterdir():
   if not d.name.isdigit():continue
   try:
    s=(d/'stat').read_text().split(') ',1)[1].split()
    if int(s[2])==p.pid:rss+=int(s[21])*os.sysconf('SC_PAGE_SIZE')
   except (OSError,ValueError,IndexError):pass
  peak=max(peak,rss)
  if time.monotonic()-start>600:status='timeout'
  elif rss>12*1024**3:status='memory_limit'
  elif shutil.disk_usage(out).free<8*1024**3:status='storage_reserve'
  elif stdout.tell()>64*1024**2 or stderr.tell()>64*1024**2:status='output_limit'
  if status!='running':
   os.killpg(p.pid,signal.SIGTERM)
   try:p.wait(timeout=5)
   except subprocess.TimeoutExpired:os.killpg(p.pid,signal.SIGKILL);p.wait()
   break
  time.sleep(.2)
 rc=p.wait()
usage=resource.getrusage(resource.RUSAGE_CHILDREN)
remaining=[]
for d in pathlib.Path('/proc').iterdir():
 if not d.name.isdigit():continue
 try:
  st=(d/'stat').read_text().split(') ',1)[1].split()
  if int(st[2])==p.pid:remaining.append(int(d.name))
 except (OSError,ValueError,IndexError):pass
if remaining:status='descendants_remaining'
r={'waited_tree_user_seconds':usage.ru_utime,'waited_tree_system_seconds':usage.ru_stime,'waited_tree_max_process_rss_kib':usage.ru_maxrss,'remaining_group_pids':remaining,'run_pid':p.pid,'input_diff_sha256':input_diff_sha256,'name':name,'args':args,'cwd':cwd,'cache':cache,'exit':rc,'elapsed_seconds':time.monotonic()-start,'status':'completed' if status=='running' else status,'observed_process_group_peak_rss_bytes':peak,'binary_sha256':hashlib.sha256(pathlib.Path(binary).read_bytes()).hexdigest(),'output_sha256':hashlib.sha256((out/(name+'.json')).read_bytes()).hexdigest()}
try:
 data=json.loads((out/(name+'.json')).read_text());r['top_keys']=list(data);r['findings']=len(data.get('findings',[]));r['analysis_complete']=data.get('analysis_outcome',{}).get('analysis_complete')
except (ValueError,OSError):r['json_valid']=False
(out/(name+'.receipt.json')).write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r))
