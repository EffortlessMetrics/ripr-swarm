import pathlib,subprocess,os,json,hashlib,datetime
p=pathlib.Path('/workspace/5500-veto-reconciliation');binary='/workspace/5500-assertion-prerequisite/ripr-c4a8f782';env=os.environ.copy();env.update(PATH='/workspace/.cargo/bin:'+env['PATH'],RUSTUP_HOME='/workspace/.rustup',CARGO_HOME='/workspace/.cargo',RUSTUP_TOOLCHAIN='1.95.0',CARGO_BUILD_JOBS='2',RAYON_NUM_THREADS='2',RUST_TEST_THREADS='2');records=[]
cases=[('closed-positive','','','600',False,'direct'),('stored-any','pub(crate) use proof::*;\n','','600',False,'direct'),('qualified-root','pub(crate) use crate::proof::*;\n','','600',False,'direct'),('owner-crate-glob','','use crate::*;\n','600',False,'direct'),('expected-glob-max','','','u64::MAX',True,'direct'),('expected-glob-numeric','','','18446744073709551615_u64',True,'direct'),('return-alias','','type Word = u64;\n','600',False,'alias'),('overwritten-actual','','','600',False,'overwrite'),('self-expected','','','600',False,'self')]
for name,rootimport,ownerprefix,expected,maxval,kind in cases:
 root=p/'cases'/name;(root/'src').mkdir(parents=True,exist_ok=True);(root/'Cargo.toml').write_text("[package]\nname='veto_witness'\nversion='0.0.0'\nedition='2024'\n[workspace]\n");(root/'src/lib.rs').write_text('mod proof;\n'+rootimport)
 returned='Word' if kind=='alias' else 'u64';actual='Some(u64::MAX)' if maxval else 'None';glob='    use crate::*;\n' if maxval else ''
 assertion=f'        assert_eq!(charge(&lease), {expected});'
 if kind=='overwrite':assertion='        let mut actual = charge(&lease);\n        actual = 600;\n        assert_eq!(actual, 600);'
 if kind=='self':assertion='        let expected = charge(&lease);\n        assert_eq!(charge(&lease), expected);'
 source=ownerprefix+f'''struct Lease {{ elapsed: Option<u64> }}
fn charge(lease: &Lease) -> {returned} {{
    match lease.elapsed {{
        Some(ms) => ms,
        None => 600,
    }}
}}
#[cfg(test)]
mod tests {{
    use super::*;
{glob}    #[test]
    fn observes() {{
        let lease = Lease {{ elapsed: {actual} }};
{assertion}
    }}
}}
''';(root/'src/proof.rs').write_text(source);diff='--- /dev/null\n+++ b/src/proof.rs\n@@ -0,0 +1,'+str(len(source.splitlines()))+' @@\n'+''.join('+'+x+'\n' for x in source.splitlines());(root/'input.diff').write_text(diff)
 e=env.copy();e['RIPR_CACHE_DIR']=str(p/'cache'/name);cmd=[binary,'check','--root',str(root),'--diff',str(root/'input.diff'),'--mode','ready','--format','json']
 with (root/'check.json').open('w') as out,(root/'check.stderr').open('w') as err:r=subprocess.run(cmd,env=e,stdout=out,stderr=err)
 assert r.returncode==0;d=json.loads((root/'check.json').read_text());headers=[x for x in d['findings'] if x['probe']['family']=='match_arm' and x['probe']['expression'].startswith('match ') and '=>' not in x['probe']['expression']];assert len(headers)==1
 h=headers[0];native=[]
 for variant in ['original','zero']:
  dest=root/'native'/variant;(dest/'src').mkdir(parents=True,exist_ok=True);(dest/'src/lib.rs').write_text((root/'src/lib.rs').read_text());s=source if variant=='original' else source.replace('Some(ms) => ms,','Some(ms) => 0,').replace('None => 600,','None => 0,');(dest/'src/proof.rs').write_text(s)
  compilecmd=['rustc','--edition=2024','--test',str(dest/'src/lib.rs'),'-o',str(dest/'test-bin')]
  with (dest/'compile.log').open('w') as out:c=subprocess.run(compilecmd,env=env,stdout=out,stderr=subprocess.STDOUT)
  assert c.returncode==0,(name,variant)
  with (dest/'run.log').open('w') as out:t=subprocess.run([str(dest/'test-bin'),'--test-threads=1'],env=env,stdout=out,stderr=subprocess.STDOUT)
  native.append({'variant':variant,'compile_exit':c.returncode,'test_exit':t.returncode,'source_sha256':hashlib.sha256(s.encode()).hexdigest()})
 records.append({'case':name,'command':cmd,'native_cli_exit':r.returncode,'analysis_complete':d['analysis_outcome']['analysis_complete'],'header_id':h['id'],'classification':h['classification'],'whole_tail_receipt':any('whole_tail_match_return_observation:' in x for x in h['evidence']),'native':native});(p/'witness-receipt.json').write_text(json.dumps({'source_head':'c4a8f7828e15426c4d872718f66bd4bac3ab0daf','binary_sha256':hashlib.sha256(pathlib.Path(binary).read_bytes()).hexdigest(),'records':records},indent=2)+'\n');print(name,h['classification'],[x['test_exit'] for x in native],flush=True)
