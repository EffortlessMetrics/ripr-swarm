import pathlib,subprocess,os,json,time,shutil,sys,hashlib
root=pathlib.Path('/workspace/ripr-swarm');out=pathlib.Path('/workspace/8004-repair');env=dict(os.environ,RUSTUP_HOME='/workspace/.rustup',CARGO_HOME='/workspace/.cargo',PATH='/workspace/.cargo/bin:'+os.environ['PATH'],CARGO_BUILD_JOBS='2')
def git(*args):return subprocess.check_output(['git',*args],cwd=root)
assert not git('status','--porcelain')
head=git('rev-parse','HEAD').decode().strip();tree=git('rev-parse','HEAD^{tree}').decode().strip();base=git('rev-parse','origin/main').decode().strip();mergebase=git('merge-base','origin/main','HEAD').decode().strip();assert base==mergebase
selected=git('diff','--name-only','-z','origin/main...HEAD');independent=git('diff-tree','--no-commit-id','--name-only','-r','-z',base,head);assert selected==independent and selected
(out/'candidate-selector.nul').write_bytes(selected);(out/'candidate-tree-selector.nul').write_bytes(independent);paths=[p.decode() for p in selected.split(b'\0') if p];(out/'candidate-selector.txt').write_text('\n'.join(paths)+'\n')
(out/'candidate-selector.json').write_text(json.dumps(dict(head=head,tree=tree,base=base,mergebase=mergebase,changed_paths=paths,count=len(paths),independent_inventory_equal=True,raw_sha256=hashlib.sha256(selected).hexdigest()),indent=2)+'\n')
commands=[('check-fast',['cargo','xtask','check-fast']),('precommit',['cargo','xtask','precommit']),('goldens',['cargo','xtask','goldens','check']),('honesty',['cargo','xtask','check-evidence-promotion-honesty']),('dogfood',['cargo','xtask','dogfood']),('corpora',['cargo','xtask','verdict-corpus','check-all'])]
results=[]
for label,argv in commands:
 free=shutil.disk_usage(root).free
 if free<8*1024**3:raise RuntimeError(('storage reserve refuses next batch',label,free))
 start_ns=time.time_ns();start=time.time()
 with (out/('candidate-'+label+'.log')).open('w') as f:run=subprocess.run(argv,cwd=root,env=env,stdout=f,stderr=subprocess.STDOUT)
 row=dict(label=label,argv=argv,head=head,tree=tree,base=base,returncode=run.returncode,seconds=round(time.time()-start,2),cargo_build_jobs=2,free_bytes_before=free,free_bytes_after=shutil.disk_usage(root).free)
 results.append(row);(out/'candidate-gates.json').write_text(json.dumps(results,indent=2)+'\n');print(json.dumps(row),flush=True)
 reports=out/('reports-'+label);reports.mkdir(exist_ok=True)
 copied=[]
 report_root=root/'target/ripr/reports'
 for p in report_root.rglob('*'):
  if p.is_file() and p.suffix in ['.md','.json'] and p.stat().st_mtime_ns>=start_ns:
   relative=p.relative_to(report_root);target=reports/relative;target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(p,target);copied.append(str(relative))
 row['reports_written_this_invocation']=copied
 (out/'candidate-gates.json').write_text(json.dumps(results,indent=2)+'\n')
 if run.returncode:sys.exit(run.returncode)
 if label=='check-fast':
  report=(root/'target/ripr/reports/check-fast-selector.md').read_text();assert f'Changed files: {len(paths)}' in report and 'Status: pass' in report
