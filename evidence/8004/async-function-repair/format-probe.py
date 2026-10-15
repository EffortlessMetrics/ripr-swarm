from pathlib import Path
import subprocess,json,hashlib,difflib,os
r=Path('/workspace/8004-repair');repo=Path('/workspace/ripr-swarm');base=(repo/'fixtures/inherent_receiver_async_function/controls/sync-chain.rs').read_text()
source=base.replace('{ fetch_as_object() }','{\n    fetch_as_object()\n}').replace('{ load_as_object() }','{\n    load_as_object()\n}')
assert source!=base
out=r/'format-probe';out.mkdir();rows=[];full=None
for fmt in ['repo-exposure-json','repo-exposure-summary-json','json']:
 d=out/fmt;(d/'src').mkdir(parents=True);(d/'src/lib.rs').write_text(source);(d/'Cargo.toml').write_text('[package]\nname="format-control"\nversion="0.0.0"\nedition="2024"\n[workspace]\n')
 (d/'diff.patch').write_text('diff --git a/src/lib.rs b/src/lib.rs\n'+''.join(difflib.unified_diff(source.replace('Self::Object(value) => Some(*value),','Self::Object(value) => None,').splitlines(True),source.splitlines(True),fromfile='a/src/lib.rs',tofile='b/src/lib.rs')))
 args=[str(r/'candidate-ripr'),'check','--root',str(d),'--mode','ready','--format',fmt]+(['--diff',str(d/'diff.patch')] if fmt=='json' else [])
 p=subprocess.run(args,capture_output=True,text=True);(d/'output.json').write_text(p.stdout);(d/'stderr.log').write_text(p.stderr);assert p.returncode==0
 data=json.loads(p.stdout)
 if fmt=='repo-exposure-json':
  full=data;s=[s for s in data['seams'] if s['owner']=='src/lib.rs::impl JsonValue::as_object'];assert len(s)==4;assert all(x['related_tests'][0]['relation_reason']=='helper_owner_call' for x in s)
 if fmt=='repo-exposure-summary-json':assert all(full['metrics'][k]==v for k,v in data['metrics']['grip_class'].items())
 rows.append(dict(argv=args,returncode=p.returncode,format=fmt))
native=out/'native';native.mkdir()
for variant,data in [('original',source),('mutant',source.replace('Self::Object(value) => Some(*value),','Self::Object(value) => None,'))]:
 p=native/(variant+'.rs');p.write_text(data);b=native/variant;args=['/workspace/.rustup/toolchains/1.95.0-x86_64-unknown-linux-gnu/bin/rustc','--edition','2024','--test',str(p),'-o',str(b)]
 c=subprocess.run(args,capture_output=True,text=True);(native/(variant+'.compile.log')).write_text(c.stdout+c.stderr);assert c.returncode==0
 run=subprocess.run([str(b)],capture_output=True,text=True);(native/(variant+'.run.log')).write_text(run.stdout+run.stderr);assert run.returncode==(0 if variant=='original' else 101)
 rows.append(dict(variant=variant,argv=args,compile_returncode=c.returncode,run_argv=[str(b)],run_returncode=run.returncode,source_sha256=hashlib.sha256(data.encode()).hexdigest()))
(out/'results.json').write_text(json.dumps(dict(head='a5c69b1cd2ef73d47a11163bca0432f9fe285c3d',baseline_case='inherent_receiver_async_function/controls/sync-chain.rs',baseline_sha256=hashlib.sha256(base.encode()).hexdigest(),format_only_source_sha256=hashlib.sha256(source.encode()).hexdigest(),records=rows),indent=2)+'\n');print('formatting-only multiline control: helper/high retained; compact parity; diff conservative; native original0/mutant101')
