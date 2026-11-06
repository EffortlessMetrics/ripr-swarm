import subprocess,os,time,signal,json,shutil,sys
from pathlib import Path
p=subprocess.Popen(['bash',sys.argv[1] if len(sys.argv)>1 else '/workspace/large-warm-proof/run-controls.sh'],start_new_session=True)
start=time.monotonic();peak=0;reason='completed'
while p.poll() is None:
 procs={}
 for item in Path('/proc').iterdir():
  if not item.name.isdigit():continue
  try:
   s=(item/'stat').read_text().rsplit(')',1)[1].split();procs[int(item.name)]=(int(s[1]),int(s[21])*os.sysconf('SC_PAGE_SIZE'))
  except (OSError,ValueError,IndexError):pass
 descendants={p.pid}
 while True:
  new={pid for pid,(ppid,_) in procs.items() if ppid in descendants}
  if new<=descendants:break
  descendants|=new
 rss=sum(procs.get(pid,(0,0))[1] for pid in descendants);peak=max(peak,rss)
 if rss>3*1024**3 or shutil.disk_usage('/workspace').free<5*1024**3 or time.monotonic()-start>750:
  reason='resource_guard';os.killpg(p.pid,signal.SIGTERM)
  try:p.wait(timeout=5)
  except subprocess.TimeoutExpired:os.killpg(p.pid,signal.SIGKILL)
  break
 time.sleep(.5)
status=p.wait()
Path('/workspace/large-warm-proof/'+(Path(sys.argv[1]).stem+'-guard.json' if len(sys.argv)>1 else 'control-guard.json')).write_text(json.dumps({'status':status,'reason':reason,'aggregate_rss_limit_bytes':3*1024**3,'guard_peak_aggregate_rss_bytes':peak,'overall_limit_seconds':750,'child_limit_seconds':90}))
raise SystemExit(status)
