import os,pathlib,subprocess,json,datetime,sys
p=pathlib.Path('/workspace/5500-assertion-prerequisite');env=os.environ.copy();env.update(PATH='/workspace/.cargo/bin:'+env['PATH'],RUSTUP_HOME='/workspace/.rustup',CARGO_HOME='/workspace/.cargo',CARGO_TARGET_DIR='/workspace/5500-target',CARGO_BUILD_JOBS='2',CARGO_INCREMENTAL='0',CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0',RUST_TEST_THREADS='2',RAYON_NUM_THREADS='2');rows=[]
commands=[('check-fast',['cargo','xtask','check-fast']),('precommit',['cargo','xtask','precommit']),('dogfood',['cargo','xtask','dogfood']),('honesty',['cargo','xtask','check-evidence-promotion-honesty'])]
for name,cmd in commands:
 start=datetime.datetime.now(datetime.timezone.utc).isoformat()
 with (p/(name+'.log')).open('w') as f:r=subprocess.run(cmd,cwd='/workspace/5500-assertion-source',env=env,stdout=f,stderr=subprocess.STDOUT)
 rows.append({'name':name,'command':cmd,'started_utc':start,'native_exit':r.returncode,'head':'c4a8f7828e15426c4d872718f66bd4bac3ab0daf'});(p/'gate-command-receipt.json').write_text(json.dumps(rows,indent=2)+'\n');print(name,r.returncode,flush=True)
 if r.returncode:sys.exit(r.returncode)
