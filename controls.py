import hashlib,json,os,pathlib,signal,subprocess
P=pathlib.Path('/workspace/scratch/probe-shape-reuse')
ROOT=pathlib.Path('/workspace/probe-shape-reuse')
env=os.environ.copy(); env.update({'PATH':'/workspace/.cargo/bin:'+env['PATH'],'CARGO_HOME':'/workspace/.cargo','RUSTUP_HOME':'/workspace/.rustup','RUSTUP_TOOLCHAIN':'1.95.0','RAYON_NUM_THREADS':'4','RIPR_REPO_EXPOSURE_LATENCY_TRACE':'1'})
fixture=P/'include-control'; (fixture/'src').mkdir(parents=True,exist_ok=True)
(fixture/'Cargo.toml').write_text('[package]\nname="include-control"\nversion="0.0.0"\nedition="2024"\n[workspace]\n')
(fixture/'src/lib.rs').write_text('include!("body.rs");\n')
(fixture/'src/body.rs').write_text('pub fn positive(value:i32)->bool {value>0}\n#[cfg(test)] mod tests {#[test] fn positive_case(){assert!(super::positive(1));}}\n')
(fixture/'change.diff').write_text('--- a/src/body.rs\n+++ b/src/body.rs\n@@ -1 +1 @@\n-pub fn positive(value:i32)->bool {value>=0}\n+pub fn positive(value:i32)->bool {value>0}\n')
cases=[('boundary',ROOT/'fixtures/boundary_gap/input',ROOT/'fixtures/boundary_gap/diff.patch'),('roles',ROOT/'fixtures/source_role_harness_suppression/input',ROOT/'fixtures/source_role_harness_suppression/diff.patch'),('include',fixture,fixture/'change.diff')]
binaries={'baseline':P/'baseline-ripr','candidate':P/'candidate-ripr'}
results=[]
for label,root,diff in cases:
    reports={}; byte_reports={}
    for variant,binary in binaries.items():
        setting=env.copy();setting['RIPR_CACHE_DIR']=str(P/f'control-cache-{label}-{variant}')
        argv=[str(binary),'check','--root',str(root),'--diff',str(diff),'--format','json','--mode','ready']
        outpath=P/f'{label}-{variant}.json';errpath=P/f'{label}-{variant}.stderr'
        with outpath.open('wb') as out,errpath.open('wb') as err:
            child=subprocess.Popen(argv,cwd=ROOT,env=setting,stdout=out,stderr=err,start_new_session=True)
            try: code=child.wait(timeout=20)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid,signal.SIGKILL); child.wait(); raise
        assert code==0,(label,variant,code)
        raw=outpath.read_bytes(); report=json.loads(raw); artifact=report.pop('artifact',None)
        byte_reports[variant]=raw;reports[variant]=report
        results.append({'case':label,'variant':variant,'argv':argv,'exit':code,'stdout_sha256':hashlib.sha256(raw).hexdigest(),'findings':len(report.get('findings',[])),'artifact_removed':artifact is not None})
    assert reports['baseline']==reports['candidate'],f'unexpected semantic delta: {label}'
    results.append({'case':label,'semantic_equal_after_only_top_level_artifact_removal':True,'exact_stdout_equal':byte_reports['baseline']==byte_reports['candidate']})
assert any(r.get('findings',0)>0 for r in results)
(P/'controls.json').write_text(json.dumps(results,indent=2)+'\n')
print(json.dumps([r for r in results if 'semantic_equal_after_only_top_level_artifact_removal' in r],indent=2))
