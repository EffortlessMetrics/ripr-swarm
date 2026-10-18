import pathlib,subprocess,os,json,hashlib
lab=pathlib.Path('/workspace/5500-review'); source=(lab/'scrutinee_return/src/lib.rs').read_text(); records=[]
env=os.environ.copy();env.update(RUSTUP_HOME='/workspace/.rustup',CARGO_HOME='/workspace/.cargo')
for name,text in [('original',source),('some-zero',source.replace('Some(ms) => ms','Some(ms) => 0')),('none-zero',source.replace('None => 600','None => 0')),('both-zero',source.replace('Some(ms) => ms','Some(ms) => 0').replace('None => 600','None => 0'))]:
    p=lab/('scrutinee-'+name+'.rs'); p.write_text(text); binary=p.with_suffix('')
    compile=subprocess.run(['/workspace/.cargo/bin/rustc','--edition=2024','--test',str(p),'-o',str(binary)],env=env,capture_output=True)
    p.with_suffix('.compile.log').write_bytes(compile.stdout+compile.stderr)
    run=subprocess.run([str(binary)],capture_output=True) if compile.returncode==0 else None
    if run: p.with_suffix('.run.log').write_bytes(run.stdout+run.stderr)
    records.append(dict(case=name,compile_exit=compile.returncode,run_exit=run.returncode if run else None,source_sha256=hashlib.sha256(text.encode()).hexdigest()))
(lab/'scrutinee-native-receipt.json').write_text(json.dumps(records,indent=2)); print(json.dumps(records,indent=2))
