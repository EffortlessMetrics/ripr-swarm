import pathlib,subprocess,json,os,sys,time,hashlib
phase=sys.argv[1]
root=pathlib.Path(sys.argv[2]) if len(sys.argv)>2 else pathlib.Path('/workspace/review8004'+('-composition' if phase=='composition' else ''))
out=pathlib.Path('/workspace/review8004-evidence')
env=dict(os.environ,CARGO_HOME='/workspace/.cargo',RUSTUP_HOME='/workspace/.rustup',PATH='/workspace/.cargo/bin:/usr/bin:/bin',CARGO_TARGET_DIR='/workspace/review8004-target',CARGO_BUILD_JOBS='4')
cmds=[('cli-build',['cargo','build','--locked','-p','ripr','--bin','ripr'])]
if phase=='source':
 cmds += [('same-line',['cargo','test','--locked','-p','ripr','--lib','same_line_helper_callers_keep_distinct_module_function_identity','--','--nocapture']),('receiver',['cargo','test','--locked','-p','ripr','--test','private_inherent_receiver','--','--test-threads=4'])]
else:
 # Source now has a confirmed blocker. Bound composition checks to its
 # executable reproduction and the affected public integration consumers.
 for label,target,flt in [('receiver','private_inherent_receiver',None),('assertion-helper','assertion_helper_credit',None),('native-path','native_path_roots',None),('suppression','cli_smoke','suppression_notice'),('structural','cli_smoke','structural_workspace_markers')]:
  cmds.append((label,['cargo','test','--locked','-p','ripr','--test',target]+([flt] if flt else [])+['--','--test-threads=4']))
results=[]
for label,argv in cmds:
 p=out/f'{phase}-{label}.log'
 start=time.time()
 with p.open('w') as f: run=subprocess.run(argv,cwd=root,env=env,stdout=f,stderr=subprocess.STDOUT)
 row={'label':label,'argv':argv,'cwd':str(root),'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'tree':subprocess.check_output(['git','rev-parse','HEAD^{tree}'],cwd=root,text=True).strip(),'exit':run.returncode,'seconds':round(time.time()-start,2)}
 results.append(row);(out/f'{phase}-checks.json').write_text(json.dumps(results,indent=2)+'\n');print(json.dumps(row),flush=True)
 if run.returncode: sys.exit(run.returncode)
 if label=='cli-build':
  binary=pathlib.Path('/workspace/review8004-target/debug/ripr')
  frozen=out/(phase+'-ripr');frozen.write_bytes(binary.read_bytes());frozen.chmod(0o755)
  (out/(phase+'-ripr.sha256')).write_text(hashlib.sha256(frozen.read_bytes()).hexdigest()+'\n')
