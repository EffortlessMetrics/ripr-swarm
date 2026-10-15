import pathlib,json,subprocess,hashlib,os
repo=pathlib.Path('/workspace/ripr-swarm')
rows=json.loads((repo/'fixtures/inherent_receiver_free_helper_identity/cases.json').read_text())+json.loads(pathlib.Path('/workspace/8004-repair/new-fixture/cases.json').read_text())
out=pathlib.Path('/workspace/8004-repair/native-matrix');out.mkdir(exist_ok=True)
results=[]
for n,row in enumerate(rows):
    relative=row['path']
    src=repo/'fixtures'/relative
    if 'inherent_receiver_async_function/' in relative:
        src=pathlib.Path('/workspace/8004-repair/new-fixture')/relative.split('/',1)[1]
    text=src.read_text(); case=out/f'{n:02d}';case.mkdir(exist_ok=True)
    assert text.count('Self::Object(value) => Some(*value)')==1, relative
    for variant,data in [('original',text),('mutant',text.replace('Self::Object(value) => Some(*value)','Self::Object(value) => None'))]:
        source=case/f'{variant}.rs';source.write_text(data); binary=case/variant
        argv=['/workspace/.rustup/toolchains/1.95.0-x86_64-unknown-linux-gnu/bin/rustc','--edition','2024','--test',str(source),'-o',str(binary)]
        compiled=subprocess.run(argv,capture_output=True,text=True)
        (case/f'{variant}.compile.log').write_text(compiled.stdout+compiled.stderr)
        assert compiled.returncode==0,(relative,variant,compiled.stderr)
        run=subprocess.run([str(binary)],capture_output=True,text=True)
        (case/f'{variant}.run.log').write_text(run.stdout+run.stderr)
        expected=0 if variant=='original' or not row['native_reaches'] else 101
        result=dict(path=relative,variant=variant,source_sha256=hashlib.sha256(data.encode()).hexdigest(),command=argv,compile_return=compiled.returncode,run_command=[str(binary)],returncode=run.returncode,expected=expected)
        results.append(result)
        (out/'results.json').write_text(json.dumps(results,indent=2)+'\n')
        assert run.returncode==expected,result
print(f'{len(rows)} cases; {len(results)} native executions; all match independent native_reaches expectations')
