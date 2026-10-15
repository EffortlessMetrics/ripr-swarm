import pathlib, subprocess, json, hashlib, os, sys
root=pathlib.Path('/workspace/review8004')
out=pathlib.Path(sys.argv[2]) if len(sys.argv)>2 else pathlib.Path('/workspace/review8004-evidence/native'+('-extras' if len(sys.argv)>1 else ''))
out.mkdir(exist_ok=True)
env=dict(os.environ,CARGO_HOME='/workspace/.cargo',RUSTUP_HOME='/workspace/.rustup',PATH='/workspace/.cargo/bin:/usr/bin:/bin')
cases=json.loads(pathlib.Path(sys.argv[1]).read_text()) if len(sys.argv)>1 else json.loads((root/'fixtures/inherent_receiver_free_helper_identity/cases.json').read_text())
results=[]
for i,c in enumerate(cases):
    d=out/f'{i:02d}';d.mkdir(exist_ok=True)
    source=(root/'fixtures'/c['path']).read_text()
    needle='Self::Object(value) => Some(*value),'
    assert source.count(needle)==1,(i,'mutation is not unique')
    row={'case':c,'source_sha256':hashlib.sha256(source.encode()).hexdigest()}
    for label,s in [('original',source),('mutant',source.replace(needle,'Self::Object(value) => None,'))]:
        p=d/f'{label}.rs';p.write_text(s)
        argv=['rustc','--edition=2024','--test','--crate-name','review_control',str(p),'-o',str(d/label)]
        run=subprocess.run(argv,env=env,capture_output=True,text=True)
        (d/f'{label}-compile.log').write_text(run.stdout+run.stderr)
        assert run.returncode==0,(i,label,'compile',run.returncode)
        run=subprocess.run([str(d/label),'--nocapture'],env=env,capture_output=True,text=True)
        (d/f'{label}-run.log').write_text(run.stdout+run.stderr)
        row[label]={'exit':run.returncode,'argv':argv}
    results.append(row)
    (out/'results.json').write_text(json.dumps(results,indent=2)+'\n')
    assert row['original']['exit']==0,(i,'original failed')
    assert (row['mutant']['exit']!=0)==c['native_reaches'],(i,'native truth differs')
print(json.dumps({'cases':len(results),'original_pass':sum(r['original']['exit']==0 for r in results),'mutant_fail':sum(r['mutant']['exit']!=0 for r in results),'mutant_survive':sum(r['mutant']['exit']==0 for r in results)}))
