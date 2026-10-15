import pathlib,subprocess,os,json,time,shutil,sys
root=pathlib.Path('/workspace/ripr-swarm');out=pathlib.Path('/workspace/8004-repair');env=dict(os.environ,RUSTUP_HOME='/workspace/.rustup',CARGO_HOME='/workspace/.cargo',PATH='/workspace/.cargo/bin:'+os.environ['PATH'],CARGO_BUILD_JOBS='2')
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip();tree=subprocess.check_output(['git','rev-parse','HEAD^{tree}'],cwd=root,text=True).strip();assert head=='a5c69b1cd2ef73d47a11163bca0432f9fe285c3d';assert not subprocess.check_output(['git','status','--porcelain'],cwd=root)
commands=[('honesty',['cargo','xtask','check-evidence-promotion-honesty'],128),('dogfood',['cargo','xtask','dogfood'],512)]+[(lang+'-corpus',['cargo','xtask','verdict-corpus','check','--language',lang],256) for lang in ['rust','python','typescript']]
results=[]
for label,args,peak_mib in commands:
 free=shutil.disk_usage(root).free;peak=peak_mib*1024**2
 if free < 8*1024**3+peak:
  row=dict(label=label,argv=args,head=head,tree=tree,outcome='not_admitted',free_bytes=free,reserve_bytes=8*1024**3,peak_additional_bytes=peak);results.append(row);(out/'remaining-checks.json').write_text(json.dumps(results,indent=2)+'\n');print(json.dumps(row),flush=True);continue
 start_ns=time.time_ns();start=time.time()
 with (out/('candidate-'+label+'.log')).open('w') as f:run=subprocess.run(args,cwd=root,env=env,stdout=f,stderr=subprocess.STDOUT)
 row=dict(label=label,argv=args,head=head,tree=tree,returncode=run.returncode,seconds=round(time.time()-start,2),free_bytes_before=free,free_bytes_after=shutil.disk_usage(root).free,peak_additional_bytes=peak)
 results.append(row);(out/'remaining-checks.json').write_text(json.dumps(results,indent=2)+'\n');print(json.dumps(row),flush=True)
 reports=out/('reports-'+label);reports.mkdir(exist_ok=True)
 rr=root/'target/ripr/reports'
 for p in rr.rglob('*'):
  if p.is_file() and p.suffix in ['.md','.json'] and p.stat().st_mtime_ns>=start_ns:
   target=reports/p.relative_to(rr);target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(p,target)
 if run.returncode:sys.exit(run.returncode)
free=shutil.disk_usage(root).free;peak=3*1024**3
row=dict(label='perl-corpus',argv=['cargo','xtask','verdict-corpus','check','--language','perl'],head=head,tree=tree,outcome='not_admitted' if free<8*1024**3+peak else 'admissible_not_started',free_bytes=free,reserve_bytes=8*1024**3,peak_additional_bytes=peak,reason='Requires a fresh lang-perl executable; cold variant peak must fit above unchanged reserve. No inherited target cleanup or lower reserve.')
results.append(row);(out/'remaining-checks.json').write_text(json.dumps(results,indent=2)+'\n');print(json.dumps(row),flush=True)
