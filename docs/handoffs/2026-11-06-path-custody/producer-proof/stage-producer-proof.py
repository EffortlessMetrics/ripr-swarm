from pathlib import Path
import shutil,hashlib,json
root=Path('/workspace/ripr-path-combined')
src=root/'xtask/src/reports'
out=Path('/workspace/ripr-docs-evidence/producer-harness/src')
base=out/'reports/release/candidate_harness'
base.mkdir(parents=True,exist_ok=True)
originals={}
for name in ['live_head','input']:
    shutil.copytree(src/'release/candidate_harness'/name,base/name,dirs_exist_ok=True)
    shutil.copyfile(src/'release/candidate_harness'/f'{name}.rs',base/f'{name}/mod.rs')
for p in [base/'live_head/tests.rs',base/'live_head/producer.rs']:
    text=p.read_text()
    removal='#[path = "source_tests.rs"]\nmod source_tests;' if p.name=='tests.rs' else '#[cfg(test)]\nmod tests;'
    assert text.count(removal)==1
    p.write_text(text.replace(removal,'// Excluded from this bounded module proof; see handoff.'))
for name in ['source_promotion_acceptance']:
    shutil.copytree(src/name,out/'reports'/name,dirs_exist_ok=True)
    shutil.copyfile(src/f'{name}.rs',out/'reports'/name/'mod.rs')
for name in ['path_custody.rs','path_inventory.rs']:
    shutil.copyfile(src/name,out/'reports'/name)
for p in (out/'reports').rglob('*.rs'):
    if p.name=='mod.rs':
        relative=str(p.relative_to(out/'reports').parent)+'.rs'
    else: relative=str(p.relative_to(out/'reports'))
    origin=src/relative
    if origin.exists():
        originals[relative]={'source_sha256':hashlib.sha256(origin.read_bytes()).hexdigest(),'staged_sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'identical':origin.read_bytes()==p.read_bytes()}
(out/'source-identities.json').write_text(json.dumps(originals,indent=2)+'\n')
safe=(src/'release/candidate_harness.rs').read_text();safe=safe[safe.index('fn safe_artifact_path'):]
(base/'mod.rs').write_text('use std::path::{Path,Component};\nmod input;\npub(crate) mod live_head;\n'+safe+'''\nmod source {
 pub(super) fn git_bytes(_: &std::path::Path,_:&[&str])->Result<Vec<u8>,String>{Err("source Git is outside this proof".into())}
}
''')
(out/'reports/release/mod.rs').write_text('pub(crate) mod candidate_harness;\n')
(out/'reports/mod.rs').write_text('''pub(crate) mod release;
pub(crate) mod path_custody;
pub(crate) mod path_inventory;
pub(crate) mod source_promotion_acceptance;
pub(crate) mod release_ledger {
 pub(crate) fn range_touched_recipe()->String {
  super::path_inventory::path_inventory(&[],"release-ledger capture: union of per-record changed paths across base..cut").path_record_recipe
 }
}
pub(crate) mod source_promotion {
 pub(crate) struct CommitRange { pub all_reachable_count:usize,pub first_parent_count:usize,pub all_reachable_sha256:String,pub first_parent_ordered_sha256:String }
 pub(crate) fn commit_range_with(_: &str,_:&str,_:impl FnMut(&[&str])->Result<String,String>)->Result<CommitRange,String>{Err("range Git is outside this proof".into())}
}
''')
(out/'lib.rs').write_text('''#![allow(dead_code,unused_imports)]
mod reports;
mod run {
 pub struct TimedBytesOutput { pub timed_out:bool,pub status:Option<std::process::ExitStatus>,pub stdout:Vec<u8>,pub stderr:Vec<u8> }
 pub struct ByteCaptureBudget {pub timeout:std::time::Duration,pub stdout_bytes:usize,pub stderr_bytes:usize}
 pub fn capture_bytes_in_dir_with_budget(_: &std::path::Path,_:&[String],_:(&std::path::Path,Option<&[u8]>),_:&[&str],_:ByteCaptureBudget,_:&str)->Result<TimedBytesOutput,String>{Err("native API transport is outside this proof".into())}
 pub struct Output {pub timed_out:bool,pub stdout_truncated:bool,pub stderr_truncated:bool,pub status:Option<std::process::ExitStatus>,pub stdout:String,pub stderr:String}
 pub fn capture_output_in_dir_with_timeout_bounded(_: &std::path::Path,_:&[String],_:&[(String,String)],_:&std::path::Path,_:std::time::Duration,_:usize,_:&str)->Result<Output,String>{Err("native API transport is outside this proof".into())}
}
''')
