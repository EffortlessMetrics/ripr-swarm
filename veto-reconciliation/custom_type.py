import pathlib,os,subprocess,json
p=pathlib.Path('/workspace/5500-veto-reconciliation');root=p/'cases/custom-glob-primitive';(root/'src').mkdir(parents=True,exist_ok=True);env=os.environ.copy();env.update(PATH='/workspace/.cargo/bin:'+env['PATH'],RUSTUP_HOME='/workspace/.rustup',CARGO_HOME='/workspace/.cargo',RUSTUP_TOOLCHAIN='1.95.0',RAYON_NUM_THREADS='2');sources={'lib.rs':'mod foreign;\nmod proof;\n','foreign.rs':'''#[allow(non_camel_case_types)]
#[derive(Debug)]
pub(crate) struct u64(pub(crate) std::primitive::u64);
impl PartialEq<i32> for u64 {
    fn eq(&self, _: &i32) -> bool { true }
}
''','proof.rs':'''use crate::foreign::*;
struct Lease { elapsed: Option<std::primitive::u64> }
fn charge(lease: &Lease) -> u64 {
    match lease.elapsed {
        Some(ms) => u64(ms),
        None => u64(600),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observes() {
        let lease = Lease { elapsed: None };
        assert_eq!(charge(&lease), 600);
    }
}
'''}
(root/'Cargo.toml').write_text("[package]\nname='veto_witness'\nversion='0.0.0'\nedition='2024'\n[workspace]\n")
for n,s in sources.items():(root/'src'/n).write_text(s)
s=sources['proof.rs'];(root/'input.diff').write_text('--- /dev/null\n+++ b/src/proof.rs\n@@ -0,0 +1,'+str(len(s.splitlines()))+' @@\n'+''.join('+'+x+'\n' for x in s.splitlines()));env['RIPR_CACHE_DIR']=str(p/'cache/custom-glob-primitive');cmd=['/workspace/5500-assertion-prerequisite/ripr-c4a8f782','check','--root',str(root),'--diff',str(root/'input.diff'),'--mode','ready','--format','json']
with (root/'check.json').open('w') as out,(root/'check.stderr').open('w') as err:r=subprocess.run(cmd,env=env,stdout=out,stderr=err)
assert r.returncode==0;d=json.loads((root/'check.json').read_text());h=[x for x in d['findings'] if x['probe']['family']=='match_arm' and x['probe']['expression'].startswith('match ') and '=>' not in x['probe']['expression']];assert len(h)==1;native=[]
for variant in ['original','zero']:
 dest=root/'native'/variant;(dest/'src').mkdir(parents=True,exist_ok=True)
 for n,s in sources.items():(dest/'src'/n).write_text(s if variant=='original' or n!='proof.rs' else s.replace('u64(ms)','u64(0)').replace('u64(600)','u64(0)'))
 with (dest/'compile.log').open('w') as o:c=subprocess.run(['rustc','--edition=2024','--test',str(dest/'src/lib.rs'),'-o',str(dest/'test-bin')],env=env,stdout=o,stderr=subprocess.STDOUT)
 assert c.returncode==0
 with (dest/'run.log').open('w') as o:t=subprocess.run([str(dest/'test-bin'),'--test-threads=1'],env=env,stdout=o,stderr=subprocess.STDOUT)
 native.append({'variant':variant,'compile_exit':c.returncode,'test_exit':t.returncode})
r={'source_head':'c4a8f7828e15426c4d872718f66bd4bac3ab0daf','case':'custom-glob-primitive','command':cmd,'native_cli_exit':0,'header_id':h[0]['id'],'classification':h[0]['classification'],'native':native,'claim':'A real customtypeimported throughowncrate glob has apparent u64 return and always-true heterogeneous equality. Bothnativeoriginalandzeropass. Owner primitive provenance must stayrefused; MacroBindingCandidates Names is not primitive authority.'};(p/'custom-type-receipt.json').write_text(json.dumps(r,indent=2)+'\n');print(r)
