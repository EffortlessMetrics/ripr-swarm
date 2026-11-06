import subprocess,os,time,signal,json,shutil,sys
from pathlib import Path
class ExistingBuild:
 def __init__(self):
  roots=[]
  for item in Path('/proc').iterdir():
   if not item.name.isdigit():continue
   try:
    args=(item/'cmdline').read_bytes().split(b'\0')
    if args[:1]==[b'timeout'] and b'900s' in args and b'cargo' in args and b'build' in args:roots.append(int(item.name))
   except OSError:pass
  if len(roots)!=1:raise RuntimeError(roots)
  self.pid=roots[0]
 def poll(self):return None if Path('/proc',str(self.pid)).exists() else 0
 def wait(self,timeout=None):return 0
p=ExistingBuild()
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
 if rss>12*1024**3 or shutil.disk_usage('/workspace').free<5*1024**3 or time.monotonic()-start>910:
  reason='resource_guard';[os.kill(pid,signal.SIGTERM) for pid in descendants if Path('/proc',str(pid)).exists()]
  try:p.wait(timeout=5)
  except subprocess.TimeoutExpired:os.killpg(p.pid,signal.SIGKILL)
  break
 time.sleep(.5)
status=p.wait()
Path('/workspace/large-warm-proof/repair-build-guard.json').write_text(json.dumps({'status':status,'reason':reason,'aggregate_rss_limit_bytes':12*1024**3,'guard_peak_aggregate_rss_bytes':peak,'overall_limit_seconds':910,'child_limit_seconds':900}))
raise SystemExit(status)
