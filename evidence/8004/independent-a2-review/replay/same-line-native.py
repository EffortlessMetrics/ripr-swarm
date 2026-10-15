import pathlib,subprocess,os,json,hashlib
p=pathlib.Path('/workspace/review8004-evidence/same-line-native');p.mkdir(exist_ok=True)
s=pathlib.Path('/workspace/review8004/fixtures/inherent_receiver_free_helper_identity/controls/same-line-callers.rs').read_text()
env=dict(os.environ,CARGO_HOME='/workspace/.cargo',RUSTUP_HOME='/workspace/.rustup',PATH='/workspace/.cargo/bin:/usr/bin:/bin')
rows=[]
for label,source in [('original',s),('mutant',s.replace('Self::Object(value) => Some(*value),','Self::Object(value) => None,'))]:
 path=p/(label+'.rs');path.write_text(source)
 argv=['rustc','--edition=2024','--test','--crate-name','same_line_control',str(path),'-o',str(p/label)]
 c=subprocess.run(argv,env=env,capture_output=True,text=True);(p/(label+'-compile.log')).write_text(c.stdout+c.stderr);assert c.returncode==0
 for name in ['tests::actual','tests::decoy']:
  a=[str(p/label),'--exact',name,'--nocapture'];c=subprocess.run(a,env=env,capture_output=True,text=True);(p/(label+'-'+name.split('::')[-1]+'.log')).write_text(c.stdout+c.stderr)
  rows.append({'label':label,'test':name,'exit':c.returncode,'argv':a})
assert [x['exit'] for x in rows]==[0,0,101,0]
(p/'results.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(rows))
