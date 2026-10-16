import subprocess,os,pathlib,json,time
p=pathlib.Path('/workspace/scratch/8210-proof'); c='/workspace/.cargo/bin/cargo'; env=dict(os.environ,RUSTUP_HOME='/workspace/.rustup',CARGO_HOME='/workspace/.cargo',CARGO_TARGET_DIR='/workspace/ripr-swarm/target'); rows=[]
commands=[('final-partial-cli',['test','--locked','-p','ripr','--test','cli_smoke','partial_preview_disclosure']),('final-exit-cli',['test','--locked','-p','ripr','--test','cli_smoke','unsupported_check_exit']),('final-source-admission-cli',['test','--locked','-p','ripr','--test','cli_smoke','python_source_admission']),('final-byte-budget-cli',['test','--locked','-p','ripr','--test','cli_smoke','findings_byte_budget']),('final-check-fast',['xtask','check-fast']),('final-precommit',['xtask','precommit'])]
commands += [('fixture-'+n,['xtask','fixtures',n]) for n in ['boundary_gap','python_boundary_gap','typescript_boundary_gap','python_disabled','typescript_disabled','typescript_parse_error_unsupported_syntax']]
for name,args in commands:
 print('START',name,flush=True);start=time.monotonic()
 with (p/(name+'.log')).open('wb') as f:o=subprocess.run([c,*args],env=env,stdout=f,stderr=subprocess.STDOUT)
 rows.append({'name':name,'argv':[c,*args],'exit':o.returncode,'elapsed_seconds':round(time.monotonic()-start,3),'head':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()});(p/'validation-receipt.json').write_text(json.dumps(rows,indent=2)); print('END',name,o.returncode,flush=True)
 if o.returncode:raise SystemExit(o.returncode)
