import os,signal,subprocess,time,json
from observer import NativeObserver,ObserverDenied
p=subprocess.Popen(['/usr/bin/sha256sum','/dev/zero'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,start_new_session=True)
results=[];o=None
try:
 time.sleep(.1);o=NativeObserver()
 for _ in range(3):
  r=o.sample(p.pid,p.pid);results.append(r);time.sleep(.05)
 assert all(len(r['ips'])>=3 for r in results),results
 print(json.dumps({'own_child_pid':p.pid,'samples':results,'pause_sum_s':o.pause_sum_s,'sample_wall_s':o.sample_wall_s,'errors':o.errors},indent=2))
except ObserverDenied as e:
 print(json.dumps({'permission_denied':str(e),'route_stopped':True}));raise
finally:
 try:os.killpg(p.pid,signal.SIGKILL)
 except ProcessLookupError:pass
 try:os.waitpid(p.pid,0)
 except ChildProcessError:pass
 p.returncode=-9
 print('owned capability-probe child reaped')
