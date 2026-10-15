import pathlib,subprocess,os,json,time,hashlib,sys
root=pathlib.Path('/workspace/ripr-swarm');out=pathlib.Path('/workspace/8004-repair');env=dict(os.environ,RUSTUP_HOME='/workspace/.rustup',CARGO_HOME='/workspace/.cargo',PATH='/workspace/.cargo/bin:'+os.environ['PATH'],CARGO_BUILD_JOBS='2')
assert not subprocess.check_output(['git','status','--porcelain'],cwd=root)
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip();tree=subprocess.check_output(['git','rev-parse','HEAD^{tree}'],cwd=root,text=True).strip()
commands=[('committed-cli-build',['cargo','build','--locked','-p','ripr','--bin','ripr','--message-format=json'])]
for label,selector in [('grip','test_grip_evidence'),('related','related_tests'),('owner-pin','owner_pin')]:commands.append((label,['cargo','test','--locked','-p','ripr','--lib',selector,'--','--test-threads=4']))
for name in ['private_inherent_receiver','assertion_helper_credit','helper_wrapper_reach','match_arm_helper_selection','native_path_roots']:commands.append((name,['cargo','test','--locked','-p','ripr','--test',name,'--','--test-threads=4']))
for label,selector in [('rerun','rerun_'),('suppression','suppression_notice'),('structural','structural_workspace_markers')]:commands.append((label,['cargo','test','--locked','-p','ripr','--test','cli_smoke',selector,'--','--test-threads=4']))
results=[]
for label,argv in commands:
 start=time.time()
 with (out/('candidate-'+label+'.log')).open('w') as f: run=subprocess.run(argv,cwd=root,env=env,stdout=f,stderr=subprocess.STDOUT)
 row=dict(label=label,argv=argv,cwd=str(root),head=head,tree=tree,returncode=run.returncode,seconds=round(time.time()-start,2),cargo_build_jobs=2)
 results.append(row);(out/'candidate-checks.json').write_text(json.dumps(results,indent=2)+'\n');print(json.dumps(row),flush=True)
 if run.returncode:sys.exit(run.returncode)
 if label=='committed-cli-build':
  digest=hashlib.sha256((root/'target/debug/ripr').read_bytes()).hexdigest();expected=hashlib.sha256((out/'fixed-ripr').read_bytes()).hexdigest();assert digest==expected,(digest,expected)
  (out/'committed-cli-identity.json').write_text(json.dumps(dict(head=head,tree=tree,sha256=digest,frozen_binary=str(out/'fixed-ripr'),artifact_json_log='candidate-committed-cli-build.log'),indent=2)+'\n')
 else:
  text=(out/('candidate-'+label+'.log')).read_text();assert 'test result: ok.' in text and '0 passed;' not in text, label
