from pathlib import Path
import os,subprocess,json
p=Path('/workspace/5500-assertion-prerequisite/native-alias');p.mkdir(exist_ok=True)
provider='''#[macro_export] macro_rules! assert_eq { ($($v:tt)*) => { () }; }
#[macro_export] macro_rules! emit_exported { ($d:tt) => { #[macro_export] macro_rules! assert_eq { ($d($d v:tt)*) => { () }; } }; }
''';(p/'opaque.rs').write_text(provider);env=os.environ.copy();env.update(RUSTUP_HOME='/workspace/.rustup',CARGO_HOME='/workspace/.cargo');c=subprocess.run(['/workspace/.cargo/bin/rustc','--edition=2024','--crate-name','opaque','--crate-type','rlib',str(p/'opaque.rs'),'-o',str(p/'libopaque.rlib')],env=env,capture_output=True);(p/'provider-compile.log').write_bytes(c.stdout+c.stderr);assert c.returncode==0
owner=(Path('/workspace/5500-assertion-prerequisite/return-local/src/proof.rs')).read_text()
cases={
'grouped_alias':('mod proof;\npub(crate) use proof::{external_alias::*};\n',owner.replace('#[cfg(test)]','pub(crate) use opaque as external_alias;\n#[cfg(test)]').replace('    use super::*;','    use super::*;\n    use super::external_alias::*;')),
'renamed_body':('mod proof;\npub(crate) use proof::*;\n',owner.replace('#[cfg(test)]','use opaque::emit_exported as println;\nfn install() { println!($); }\n#[cfg(test)]').replace('    use super::*;','    use super::*;\n    use crate::*;')),
}
rows=[]
for name,(root,source) in cases.items():
 for variant in ['original','zero']:
  q=p/name/variant;(q/'src').mkdir(parents=True,exist_ok=True);(q/'src/lib.rs').write_text(root);(q/'src/proof.rs').write_text(source if variant=='original' else source.replace('    600\n','    0\n'));cmd=['/workspace/.cargo/bin/rustc','--edition=2024','--test',str(q/'src/lib.rs'),'--extern','opaque='+str(p/'libopaque.rlib'),'-o',str(q/'test')];c=subprocess.run(cmd,env=env,capture_output=True);(q/'compile.log').write_bytes(c.stdout+c.stderr);t=None
  if c.returncode==0:
   t=subprocess.run([str(q/'test')],env=env,capture_output=True);(q/'run.log').write_bytes(t.stdout+t.stderr)
  rows.append(dict(case=name,variant=variant,compile_exit=c.returncode,test_exit=t.returncode if t else None))
(p/'receipt.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(rows,indent=2))
