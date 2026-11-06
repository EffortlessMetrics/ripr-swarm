import pathlib,subprocess,sys,os,json,hashlib
state=sys.argv[1];root=pathlib.Path(sys.argv[2] if len(sys.argv)>2 else '/workspace/ripr-perf-corpus');out=pathlib.Path('/workspace/ripr-perf-evidence')
files=['crates/ripr/src/analysis/classify/field_reach.rs','crates/ripr/src/analysis/language/typescript/gap_reach.rs'];times={p:(root/p).stat() for p in files}
for p in files:
 (root/p).write_bytes(subprocess.check_output(['git','show','b51aebc7e4735529befd677ebf14071c7f09e477:'+p],cwd=root))
p=root/files[0];s=p.read_text();s=s.replace('if self.struct_counts.get(type_name) != Some(&1) {','if self.struct_counts.get(type_name) != Some(&'+('3' if state in ('source','root') else '2')+') {');p.write_text(s)
if state!='original':
 p=root/files[1];s=p.read_text();a=s.index('fn a_barrel_reaches_the_changed_file_it_re_exports()');b=s.index('\n    #[test]',a);part=s[a:b];old='assert!(check(&entries, &["src/changed.ts"], &[], "test/a.test.ts"));';assert part.count(old)==1;p.write_text(s[:a]+part.replace(old,old.replace('assert!(check','assert!(!check'))+s[b:])
c=root/'ripr.toml'
if state in ('config','source','root'):c.write_text('[severity.findings]\nweakly_exposed = "info"\n')
elif c.exists():c.unlink()
for p,t in times.items():os.utime(root/p,ns=(t.st_atime_ns,t.st_mtime_ns))
diff=out/('product2.diff' if state in ('source','root') else 'product1.diff')
r={'state':state,'root':str(root),'diff':str(diff),'diff_sha256':hashlib.sha256(diff.read_bytes()).hexdigest(),'files':{p:hashlib.sha256((root/p).read_bytes()).hexdigest() for p in files},'config':c.read_text() if c.exists() else None,'mtime_restored':True}
(out/('state-'+state+'.json')).write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r))
