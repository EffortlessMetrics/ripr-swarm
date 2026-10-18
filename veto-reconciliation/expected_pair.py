import pathlib,json,subprocess,os,hashlib
p=pathlib.Path('/workspace/5500-veto-reconciliation');env=os.environ.copy();env.update(PATH='/workspace/.cargo/bin:'+env['PATH'],RUSTUP_HOME='/workspace/.rustup',CARGO_HOME='/workspace/.cargo',RUSTUP_TOOLCHAIN='1.95.0',RAYON_NUM_THREADS='2');binary='/workspace/5500-assertion-prerequisite/ripr-c4a8f782';records=[]
for name,expected in [('isolated-expected-max','u64::MAX'),('isolated-expected-numeric','18446744073709551615_u64')]:
 root=p/'cases'/name;(root/'src').mkdir(parents=True,exist_ok=True);(root/'Cargo.toml').write_text("[package]\nname='veto_witness'\nversion='0.0.0'\nedition='2024'\n[workspace]\n");sources={'lib.rs':'mod proof;\n#[cfg(test)]\nmod tests;\n','proof.rs':'''pub(crate) struct Lease { pub(crate) elapsed: Option<u64> }
pub(crate) fn charge(lease: &Lease) -> u64 {
    match lease.elapsed {
        Some(ms) => ms,
        None => 600,
    }
}
''','tests.rs':f'''use crate::proof::{{charge, Lease}};
use crate::*;
#[test]
fn observes() {{
    let lease = Lease {{ elapsed: Some(u64::MAX) }};
    assert_eq!(charge(&lease), {expected});
}}
'''}
 for f,s in sources.items():(root/'src'/f).write_text(s)
 source=sources['proof.rs'];(root/'input.diff').write_text('--- /dev/null\n+++ b/src/proof.rs\n@@ -0,0 +1,'+str(len(source.splitlines()))+' @@\n'+''.join('+'+x+'\n' for x in source.splitlines()))
 e=env.copy();e['RIPR_CACHE_DIR']=str(p/'cache'/name);cmd=[binary,'check','--root',str(root),'--diff',str(root/'input.diff'),'--mode','ready','--format','json']
 with (root/'check.json').open('w') as o,(root/'check.stderr').open('w') as err:c=subprocess.run(cmd,env=e,stdout=o,stderr=err)
 assert c.returncode==0;d=json.loads((root/'check.json').read_text());h=[x for x in d['findings'] if x['probe']['family']=='match_arm' and x['probe']['expression'].startswith('match ') and '=>' not in x['probe']['expression']];assert len(h)==1;h=h[0];native=[]
 for variant in ['original','zero']:
  dest=root/'native'/variant;(dest/'src').mkdir(parents=True,exist_ok=True)
  for f,s in sources.items():(dest/'src'/f).write_text(s if variant=='original' or f!='proof.rs' else s.replace('Some(ms) => ms,','Some(ms) => 0,').replace('None => 600,','None => 0,'))
  with (dest/'compile.log').open('w') as out:c=subprocess.run(['rustc','--edition=2024','--test',str(dest/'src/lib.rs'),'-o',str(dest/'test-bin')],env=env,stdout=out,stderr=subprocess.STDOUT)
  assert c.returncode==0
  with (dest/'run.log').open('w') as out:t=subprocess.run([str(dest/'test-bin'),'--test-threads=1'],env=env,stdout=out,stderr=subprocess.STDOUT)
  native.append({'variant':variant,'compile_exit':c.returncode,'test_exit':t.returncode})
 records.append({'case':name,'command':cmd,'native_cli_exit':0,'analysis_complete':d['analysis_outcome']['analysis_complete'],'header_id':h['id'],'classification':h['classification'],'whole_tail_receipt':any('whole_tail_match_return_observation:' in x for x in h['evidence']),'native':native});print(name,h['classification'],native,flush=True)
(p/'isolated-expected-receipt.json').write_text(json.dumps({'source_head':'c4a8f7828e15426c4d872718f66bd4bac3ab0daf','records':records},indent=2)+'\n')
