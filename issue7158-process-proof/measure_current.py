import hashlib,json,os,pathlib,signal,subprocess,time
W=pathlib.Path('/workspace/issue7158-process-repair')
E=pathlib.Path('/workspace/issue7158-process-evidence')
HEAD='21fada8b88428a0ea90216e590dd3304fcbec1f1'
env=dict(os.environ,PATH='/workspace/.ripr-toolchain/cargo/bin:'+os.environ['PATH'],CARGO_HOME='/workspace/.ripr-toolchain/cargo',RUSTUP_HOME='/workspace/.ripr-toolchain/rustup',CARGO_INCREMENTAL='0',CARGO_BUILD_JOBS='1',CARGO_NET_OFFLINE='true')
records=[]
for command,slug in [('check-workflows','workflows'),('check-agent-skills','skills')]:
    target=pathlib.Path('/workspace/issue7158-process-cold-'+slug)
    assert not target.exists(), 'cold target already exists; do not relabel warm proof'
    for state in ['cold','warm']:
        assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=W,text=True).strip()==HEAD
        name='current-'+slug+'-'+state
        args=['cargo','run','--locked','--offline','-p','repo-policy','--message-format=json','--',command]
        start=time.monotonic()
        with (E/(name+'.stdout')).open('wb') as out,(E/(name+'.stderr')).open('wb') as err:
            p=subprocess.Popen(args,cwd=W,env=dict(env,CARGO_TARGET_DIR=str(target)),stdout=out,stderr=err,start_new_session=True)
            try: p.wait(timeout=180)
            except subprocess.TimeoutExpired:
                os.killpg(p.pid,signal.SIGTERM)
                try: p.wait(timeout=15)
                except subprocess.TimeoutExpired: os.killpg(p.pid,signal.SIGKILL);p.wait()
                raise
        elapsed=time.monotonic()-start
        raw=(E/(name+'.stdout')).read_bytes(); events=[]; policy=[]; compiler=[]
        for line in raw.splitlines(keepends=True):
            try: event=json.loads(line)
            except json.JSONDecodeError: policy.append(line); continue
            events.append(event);compiler.append(line)
        units=[x for x in events if x.get('reason')=='compiler-artifact']
        selected=[x for x in units if x.get('executable') and x.get('target',{}).get('name')=='repo-policy']
        assert len(selected)==1
        names=sorted(set(x['target']['name'] for x in units))
        assert not any(x=='ripr' or x.startswith(('ra_ap_','oxc')) for x in names)
        assert p.returncode==0
        r=dict(head=HEAD,command=args,state=state,target=str(target),native_exit=p.returncode,wall_seconds=elapsed,nonfresh_compiler_units=sum(not x['fresh'] for x in units),fresh_compiler_units=sum(x['fresh'] for x in units),compiler_targets=names,compiler_json_bytes=sum(map(len,compiler)),policy_stdout_bytes=sum(map(len,policy)),stderr_bytes=(E/(name+'.stderr')).stat().st_size,executable=selected[0]['executable'],executable_sha256=hashlib.sha256(pathlib.Path(selected[0]['executable']).read_bytes()).hexdigest(),conditions='jobs1; private initially absent target for cold; shared populated dependency download cache; offline; no product profiles changed')
        assert (r['nonfresh_compiler_units']>0 if state=='cold' else r['nonfresh_compiler_units']==0)
        records.append(r);(E/'current-costs.json').write_text(json.dumps(records,indent=2)+'\n');print(json.dumps(r),flush=True)
