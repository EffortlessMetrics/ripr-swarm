import pathlib,subprocess,json,hashlib,os,time,statistics
P=pathlib.Path('/workspace/scratch/module-scope-walk');R=pathlib.Path('/workspace/module-scope-walk');old=(P/'test-first-ra.rs').read_text();new=(R/'crates/ripr/src/analysis/syntax/ra.rs').read_text()
def part(s,start,end):return s[s.index(start):s.index(end,s.index(start))]
types=part(new,'#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]','/// One row per fn,')
types=types.replace(', serde::Serialize, serde::Deserialize','').replace('#[serde(into = "ModuleItemScopesWire", from = "ModuleItemScopesWire")]\n','')
before=part(old,'pub(crate) fn module_item_scopes_in(','\n/// How many times');after=part(new,'pub(crate) fn module_item_scopes_in(','\n/// How many times');line=part(new,'pub(crate) struct LineIndex {','pub(crate) fn slice_text(');predicate=part(new,'fn enclosed_by_nested_fn_like(','\nfn extract_parser_probe_shapes(')
main=r'''
fn main() -> Result<(), Box<dyn std::error::Error>> {
 let text = std::fs::read_to_string(std::env::args().nth(1).ok_or("input")?)?;
 let parse = SourceFile::parse(&text, Edition::CURRENT); assert!(parse.errors().is_empty());
 let tree=parse.tree();let lines=LineIndex::new(&text);let expected=baseline_scopes(&tree,&lines);
 assert_eq!(candidate_scopes(&tree,&lines),expected);
 for pair in 0..4 {
  let order=if pair%2==0 {["baseline","candidate"]}else{["candidate","baseline"]};
  for variant in order {
   let mut elapsed=0.0;
   for _ in 0..3 {
    let start=std::time::Instant::now();
    let result=if variant=="baseline" {baseline_scopes(std::hint::black_box(&tree),&lines)}else{candidate_scopes(std::hint::black_box(&tree),&lines)};
    elapsed+=start.elapsed().as_secs_f64();
    assert_eq!(result,expected);std::hint::black_box(result);
   }
   println!("{{\"pair\":{pair},\"variant\":\"{variant}\",\"iterations\":3,\"elapsed_s\":{elapsed},\"function_rows\":{}}}",expected.bound_names.len());
  }
 }
 Ok(())
}
'''
source='use ra_ap_syntax::{AstNode,TextSize,SourceFile,Edition,ast::{self,HasName}};\nuse std::collections::BTreeMap;\n'+types+before.replace('module_item_scopes_in(','baseline_scopes(',1)+after.replace('module_item_scopes_in(','candidate_scopes(',1)+line+predicate+main
(P/'collector-micro.rs').write_text(source)
artifacts=[json.loads(l) for l in (P/'candidate-build.stdout').read_text().splitlines() if l.startswith('{')];ra=next(x for x in artifacts if x.get('reason')=='compiler-artifact' and x['target']['name']=='ra_ap_syntax');rlib=next(x for x in ra['filenames'] if x.endswith('.rlib'));argv=['/workspace/.cargo/bin/rustc','--edition=2024','--crate-name','collector_micro','-Copt-level=0','-Cdebuginfo=line-tables-only','-Cdebug-assertions=yes','-Coverflow-checks=yes','--extern','ra_ap_syntax='+rlib,'-L','dependency=/workspace/ripr-swarm/target/debug/deps',str(P/'collector-micro.rs'),'-o',str(P/'collector-micro')];env=os.environ.copy();env.update(CARGO_HOME='/workspace/.cargo',RUSTUP_HOME='/workspace/.rustup',RUSTUP_TOOLCHAIN='1.95.0');beg=time.monotonic()
with (P/'micro-build.stdout').open('w') as out,(P/'micro-build.stderr').open('w') as err:code=subprocess.call(argv,env=env,cwd=R,stdout=out,stderr=err)
r={'compile_argv':argv,'compile_exit':code,'compile_elapsed_s':time.monotonic()-beg,'source_sha256':hashlib.sha256(source.encode()).hexdigest(),'baseline_function_sha256':hashlib.sha256(before.encode()).hexdigest(),'candidate_function_sha256':hashlib.sha256(after.encode()).hexdigest(),'adaptation':'Exact collector/LineIndex/predicate bodies; rename collector symbols, omit only serde derives/wire attribute from identical field/type struct. Isolated collector timing, not production CLI timing.','ra_artifact':ra}
(P/'micro-receipt.json').write_text(json.dumps(r,indent=2)+'\n');assert code==0,r
with (P/'micro.stdout').open('w') as out,(P/'micro.stderr').open('w') as err:
 try:run=subprocess.run([str(P/'collector-micro'),str(P/'workload/src/lib.rs')],env=env,cwd=R,stdout=out,stderr=err,timeout=30);r['run_exit']=run.returncode
 except subprocess.TimeoutExpired:r['run_exit']='timeout30';raise
 finally:(P/'micro-receipt.json').write_text(json.dumps(r,indent=2)+'\n')
assert run.returncode==0;r['binary_sha256']=hashlib.file_digest((P/'collector-micro').open('rb'),'sha256').hexdigest();rows=[json.loads(l) for l in (P/'micro.stdout').read_text().splitlines()];assert len(rows)==8;r['rows']=rows;r['median_per_iteration_s']={v:statistics.median(x['elapsed_s']/x['iterations'] for x in rows if x['variant']==v) for v in ['baseline','candidate']};r['median_reduction_percent']=100*(1-r['median_per_iteration_s']['candidate']/r['median_per_iteration_s']['baseline']);(P/'micro-receipt.json').write_text(json.dumps(r,indent=2)+'\n');print(json.dumps({k:r[k] for k in ['compile_exit','compile_elapsed_s','run_exit','median_per_iteration_s','median_reduction_percent']},indent=2))
