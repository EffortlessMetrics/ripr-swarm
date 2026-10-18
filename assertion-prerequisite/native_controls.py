from pathlib import Path
import subprocess,os,json,hashlib
p=Path('/workspace/5500-assertion-prerequisite');src=(p/'return-local/src/proof.rs').read_text();root=(p/'return-local/src/lib.rs').read_text();custom='macro_rules! assert_eq { ($($value:tt)*) => { () }; }\npub(crate) use assert_eq;\n';rows=[]
env=os.environ.copy();env.update(RUSTUP_HOME='/workspace/.rustup',CARGO_HOME='/workspace/.cargo')
for kind,owner in [('standard',src),('custom',src.replace('#[cfg(test)]',custom+'#[cfg(test)]'))]:
 for variant in ['original','selected-zero']:
  body=owner if variant=='original' else owner.replace('    600\n','    0\n');q=p/'native'/kind/variant;(q/'src').mkdir(parents=True,exist_ok=True);(q/'src/lib.rs').write_text(root);(q/'src/proof.rs').write_text(body)
  c=subprocess.run(['/workspace/.cargo/bin/rustc','--edition=2024','--test',str(q/'src/lib.rs'),'-o',str(q/'test')],env=env,capture_output=True);(q/'compile.log').write_bytes(c.stdout+c.stderr);assert c.returncode==0
  t=subprocess.run([str(q/'test')],env=env,capture_output=True);(q/'run.log').write_bytes(t.stdout+t.stderr);want=101 if(kind,variant)==('standard','selected-zero') else 0;assert t.returncode==want
  rows.append(dict(case=kind,variant=variant,compile_exit=c.returncode,test_exit=t.returncode,owner_sha256=hashlib.sha256(body.encode()).hexdigest()))
(p/'native-controls-receipt.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(rows,indent=2))
