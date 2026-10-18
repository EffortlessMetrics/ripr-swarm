from pathlib import Path
import json,subprocess,os,hashlib
p=Path('/workspace/5500-assertion-prerequisite');binary=Path('/workspace/5500-macro-repair/ripr-final-656d6d7ef')
owner='''pub(crate) fn charge(input: bool) -> u64 {
    match input {
        true => 600,
        false => 2,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observed() { assert_eq!(charge(true), 600); }
}
'''
records=[]
for name,root in [('local','mod proof;\npub(crate) use proof::*;\n'),('qualified','mod proof;\npub(crate) use crate::proof::*;\n')]:
 q=p/name;(q/'src').mkdir(parents=True,exist_ok=True);(q/'Cargo.toml').write_text('[package]\nname="local_glob_witness"\nversion="0.1.0"\nedition="2024"\n');(q/'src/lib.rs').write_text(root);(q/'src/proof.rs').write_text(owner);diff='--- /dev/null\n+++ b/src/proof.rs\n@@ -0,0 +1,'+str(len(owner.splitlines()))+' @@\n'+''.join('+'+s+'\n' for s in owner.splitlines());(q/'input.diff').write_text(diff)
 env=os.environ.copy();env.update(RIPR_CACHE_DIR=str(p/('cache-'+name)),RAYON_NUM_THREADS='2',RUSTUP_HOME='/workspace/.rustup',CARGO_HOME='/workspace/.cargo')
 cmd=[str(binary),'check','--root',str(q),'--diff',str(q/'input.diff'),'--mode','ready','--format','json'];r=subprocess.run(cmd,capture_output=True,env=env);(q/'check.json').write_bytes(r.stdout);(q/'check.stderr').write_bytes(r.stderr);assert r.returncode==0
 d=json.loads(r.stdout);h=[f for f in d['findings'] if f['probe']['expression']=='match input'];assert len(h)==1
 c=subprocess.run(['/workspace/.cargo/bin/rustc','--edition=2024','--test',str(q/'src/lib.rs'),'-o',str(q/'test')],capture_output=True,env=env);(q/'compile.log').write_bytes(c.stdout+c.stderr);assert c.returncode==0;t=subprocess.run([str(q/'test')],capture_output=True,env=env);(q/'run.log').write_bytes(t.stdout+t.stderr);assert t.returncode==0
 records.append(dict(case=name,native_cli_exit=r.returncode,header_id=h[0]['id'],classification=h[0]['classification'],native_compile_exit=c.returncode,native_test_exit=t.returncode,root_sha256=hashlib.sha256(root.encode()).hexdigest(),owner_sha256=hashlib.sha256(owner.encode()).hexdigest()))
(p/'baseline-control-receipt.json').write_text(json.dumps(records,indent=2)+'\n');print(json.dumps(records,indent=2))
