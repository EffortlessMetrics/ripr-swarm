# Review experiment driver

```python
import sys,subprocess,time,json,os,pathlib
p=pathlib.Path('/workspace/8201-evidence'); label=sys.argv[1]; args=sys.argv[2:]
env=os.environ.copy();env.update(CARGO_HOME='/workspace/.cargo',RUSTUP_HOME='/workspace/.rustup',CARGO_TARGET_DIR='/workspace/ripr-swarm/target',CARGO_INCREMENTAL='0',PATH='/workspace/.cargo/bin:'+env['PATH'])
t=time.time()
with (p/'raw'/f'{label}.log').open('wb') as f:
 r=subprocess.run(args,env=env,stdout=f,stderr=subprocess.STDOUT)
v=dict(label=label,args=args,cwd=os.getcwd(),started=t,elapsed=time.time()-t,exit=r.returncode,head=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip())
(p/'raw'/f'{label}.json').write_text(json.dumps(v,indent=2));print(json.dumps(v));sys.exit(r.returncode)

```
