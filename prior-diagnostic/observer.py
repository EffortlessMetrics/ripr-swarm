import ctypes as C,errno,os,time
class ObserverDenied(RuntimeError):pass
class NativeObserver:
 def __init__(self):
  self.libc=C.CDLL(None,use_errno=True);self.libc.ptrace.argtypes=[C.c_ulong,C.c_ulong,C.c_void_p,C.c_void_p];self.libc.ptrace.restype=C.c_long
  self.u=C.CDLL('/lib/x86_64-linux-gnu/libunwind-x86_64.so.8',mode=C.RTLD_GLOBAL);self.p=C.CDLL('/lib/x86_64-linux-gnu/libunwind-ptrace.so.0',mode=C.RTLD_GLOBAL)
  self.p._UPT_create.argtypes=[C.c_int];self.p._UPT_create.restype=C.c_void_p;self.p._UPT_destroy.argtypes=[C.c_void_p]
  self.u._Ux86_64_create_addr_space.argtypes=[C.c_void_p,C.c_int];self.u._Ux86_64_create_addr_space.restype=C.c_void_p
  self.u._Ux86_64_init_remote.argtypes=[C.c_void_p,C.c_void_p,C.c_void_p];self.u._Ux86_64_init_remote.restype=C.c_int
  self.u._Ux86_64_step.argtypes=[C.c_void_p];self.u._Ux86_64_step.restype=C.c_int
  self.u._Ux86_64_get_reg.argtypes=[C.c_void_p,C.c_int,C.POINTER(C.c_ulonglong)];self.u._Ux86_64_get_reg.restype=C.c_int
  access=C.addressof((C.c_char*1).in_dll(self.p,'_UPT_accessors'));self.space=self.u._Ux86_64_create_addr_space(access,0)
  if not self.space:raise RuntimeError('unwind address-space creation failed')
  self.attached=set();self.root_exited=None;self.pause_sum_s=0.;self.sample_wall_s=0.;self.errors=[]
 def ptrace(self,op,tid,forward_signal=0):
  C.set_errno(0);v=self.libc.ptrace(op,tid,None,C.c_void_p(forward_signal))
  if v<0:raise OSError(C.get_errno(),os.strerror(C.get_errno()))
 def sample(self,tid,root_pid,max_frames=32):
  started=time.monotonic();paused=None;ui=None;stopped=False;forward_signal=0
  try:
   if tid not in self.attached:
    self.ptrace(0x4206,tid);self.attached.add(tid) # SEIZE: no signal injection or option changes
   paused=time.monotonic();self.ptrace(0x4207,tid) # INTERRUPT own tracee
   until=time.monotonic()+.20
   while True:
    got,status,usage=os.wait4(tid,os.WNOHANG|0x40000000)
    if got:
     if os.WIFSTOPPED(status):
      stopped=True
      if status>>16!=128:
       forward_signal=os.WSTOPSIG(status)
       return {'tid':tid,'ips':[],'error':'natural signal stop forwarded:'+str(forward_signal)}
      break
     if os.WIFEXITED(status) or os.WIFSIGNALED(status):
      self.attached.discard(tid)
      if tid==root_pid:self.root_exited=(status,usage)
      return {'tid':tid,'ips':[],'error':'thread exited'}
    if time.monotonic()>=until:raise RuntimeError('own-thread stop deadline exceeded')
    time.sleep(.001)
   cursor=(C.c_ulonglong*512)();ui=self.p._UPT_create(tid)
   if not ui:raise RuntimeError('UPT_create failed')
   result=self.u._Ux86_64_init_remote(C.byref(cursor),self.space,ui)
   if result<0:return {'tid':tid,'ips':[],'error':'init_remote:'+str(result)}
   ips=[];step=1
   for _ in range(max_frames):
    ip=C.c_ulonglong();result=self.u._Ux86_64_get_reg(C.byref(cursor),16,C.byref(ip))
    if result<0 or not ip.value:break
    ips.append(ip.value);step=self.u._Ux86_64_step(C.byref(cursor))
    if step<=0:break
   return {'tid':tid,'ips':ips,'step_result':step,'truncated':len(ips)==max_frames}
  except OSError as e:
   if e.errno in [errno.EPERM,errno.EACCES]:raise ObserverDenied(f'ptrace denied for own tid{tid}: {e}')
   if e.errno not in [errno.ESRCH,errno.ECHILD]:self.errors.append(str(e))
   return {'tid':tid,'ips':[],'error':str(e)}
  finally:
   if ui:self.p._UPT_destroy(ui)
   if stopped:
    try:
     self.ptrace(17,tid,forward_signal) # DETACH immediately; forward natural signals unchanged
     self.attached.discard(tid)
    except OSError as e:
     if e.errno!=errno.ESRCH:self.errors.append('resume:'+str(e))
   if paused is not None:self.pause_sum_s+=time.monotonic()-paused
   self.sample_wall_s+=time.monotonic()-started
