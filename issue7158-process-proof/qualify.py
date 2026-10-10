import hashlib, json, os, pathlib, signal, subprocess, time
W = pathlib.Path('/workspace/issue7158-process-repair')
E = pathlib.Path('/workspace/issue7158-process-evidence')
T = pathlib.Path('/workspace/issue7158-process-target')
HEAD = '21fada8b88428a0ea90216e590dd3304fcbec1f1'
env = dict(os.environ, PATH='/workspace/.ripr-toolchain/cargo/bin:'+os.environ['PATH'], CARGO_HOME='/workspace/.ripr-toolchain/cargo', RUSTUP_HOME='/workspace/.ripr-toolchain/rustup', CARGO_TARGET_DIR=str(T), CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='2', CARGO_NET_OFFLINE='true')
records = []
def run(name, args, bound=2400, extra=None):
    assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=W,text=True).strip() == HEAD
    start=time.monotonic()
    with (E/(name+'.stdout')).open('wb') as out, (E/(name+'.stderr')).open('wb') as err:
        p=subprocess.Popen(args,cwd=W,env=dict(env,**(extra or {})),stdout=out,stderr=err,start_new_session=True)
        timed_out=False
        resource_stop=False
        while p.poll() is None:
            if time.monotonic()-start>bound or os.statvfs('/workspace').f_bavail*os.statvfs('/workspace').f_frsize<12884901888:
                timed_out=time.monotonic()-start>bound
                resource_stop=not timed_out
                os.killpg(p.pid,signal.SIGTERM)
                try: p.wait(timeout=15)
                except subprocess.TimeoutExpired: os.killpg(p.pid,signal.SIGKILL); p.wait()
                break
            time.sleep(1)
    r=dict(name=name,command=args,head=HEAD,native_exit=p.returncode,elapsed_seconds=time.monotonic()-start,timeout=timed_out,resource_stop=resource_stop,stdout_bytes=(E/(name+'.stdout')).stat().st_size,stderr_bytes=(E/(name+'.stderr')).stat().st_size)
    records.append(r)
    (E/'qualification.json').write_text(json.dumps(records,indent=2)+'\n')
    print(json.dumps(r),flush=True)
    if p.returncode != 0 or timed_out or resource_stop: raise SystemExit(1)
run('xtask-warm-build',['cargo','build','-p','xtask','--locked','--offline','--message-format=json'],600)
artifacts=[json.loads(x) for x in (E/'xtask-warm-build.stdout').read_text().splitlines() if x.startswith('{')]
chosen=[x for x in artifacts if x.get('reason')=='compiler-artifact' and x.get('target',{}).get('name')=='xtask' and 'bin' in x.get('target',{}).get('kind',[]) and x.get('executable')]
assert len(chosen)==1 and chosen[0]['manifest_path']==str(W/'xtask/Cargo.toml')
binary=pathlib.Path(chosen[0]['executable'])
assert binary==T/'debug/xtask'
(E/'current-artifact.json').write_text(json.dumps(dict(head=HEAD,artifact=chosen[0],sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),initial_build_native_exit='NOT_ESTABLISHED: previous session unavailable; Finished terminal and warm native rerun retained'),indent=2)+'\n')
run('current-process-policy',[str(binary),'check-process-policy'],300)
run('current-policy-preflight',['cargo','policy','preflight'],300)
run('current-check-fast',[str(binary),'check-fast'],1800)
run('current-precommit',[str(binary),'precommit'],2400,{'RIPR_POLICY_PREFLIGHT_RECEIPT':'target/ripr/reports/policy-preflight.json'})
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=W,text=True).strip()==HEAD
print('QUALIFICATION_NATIVE_PASS',flush=True)
