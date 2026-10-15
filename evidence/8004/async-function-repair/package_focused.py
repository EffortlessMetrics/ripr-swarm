import pathlib,tarfile,shutil,json,hashlib,datetime
r=pathlib.Path('/workspace/8004-repair');o=r/'artifact';o.mkdir(exist_ok=True)
with tarfile.open(o/'native-additional.tar.gz','w:gz') as t:
 for name in ['native-additional','new-fixture']:
  for p in sorted((r/name).rglob('*')):
   if p.is_file() and p.suffix:t.add(p,arcname=str(p.relative_to(r)))
with tarfile.open(o/'cli-reports.tar.gz','w:gz') as t:
 for name in ['red-cli','red-cli-supported','green-cli','fixed-cli','candidate-cli','candidate-same-line']:
  for p in sorted((r/name).rglob('*')):
   if p.is_file() and 'target' not in p.relative_to(r/name).parts and p.name in ['Cargo.toml','lib.rs','diff.patch','output.json','stderr.log','results.json','violations.json']:t.add(p,arcname=str(p.relative_to(r)))
exclude_prefix=['candidate-check-fast','candidate-gates']
for p in sorted(r.iterdir()):
 if p.is_file() and p.suffix in ['.py','.json','.jsonl','.log','.txt','.md','.sha256','.patch','.tree','.exit'] and not any(p.name.startswith(n) for n in exclude_prefix):shutil.copyfile(p,o/p.name)
for group in ['initial','native','cli']:
 d=o/'resources'/group;d.mkdir(parents=True,exist_ok=True)
 for name in ['manifest.json','verification.json']:shutil.copyfile(r/('verified-quarantine-'+group)/name,d/name)
(o/'README.md').write_text('''# #8004 async-function repair: focused proof checkpoint

Product head `a5c69b1cd2ef73d47a11163bca0432f9fe285c3d`, tree `cc498e204e08facb92cab0d1dad36b36bddd428d`, normally pushed to the existing open Draft. No Ready/merge clearance. Parent assigns independent exact-head review after the final handoff.

The repair reuses the complete-source async declaration authority; indexed function-body slices start at `fn` and omit `async`. Helper traversal admits only an identified synchronous declaration before entry, intermediate or last-hop execution credit. Existing owner-return pin fallback behavior is preserved. No polling/type/value-flow/macro expansion is added. #8005 remains separate and unbenchmarked.

Executed source/composition: history-preserving merge `6b38d4e74623743d851f5dc7756725662a250e2b`, tree `834b90117415f3afc54753576fcf83ae39d727a3`, parents a2b489b9d7d5b78daebf9fc1b3e4201dae3c9524 and frozen main 98c8f172bfd6aa2b808f63431ed4eac9c224c102. Complete keyed corpus reconciliation 297/316/298 ->317; no divergent rows and all metadata retained. Repair adds eight new cases ->325 without altering any existing complete row. The later main 758a7c082c1c140d1d818bb3d7e5be78643d3f22 was read: one unrelated TypeScript __proto__ fix in three files; it was not restacked or executed here.

`adjudication.json` and `committed-cli-identity.json` bind 50 cases (all original34 unchanged,16 new),150 full/fresh compact/diff reports and the clean committed executable. Same-line caller controls add3 reports. Native input digests match the production fixtures:50 originals pass,22 mutants fail,28 survive (100 distinct matrix executions). Raw initial46 and subsequent47 native stages remain retained separately; `native-additional.tar.gz` supplies the last three cases. Native truth is independent of bounded static admission.

All12 selected command groups return0 on a5: build;433 grip,290 related-call and228 owner-pin library selections;9 receiver,23 assertion-helper,33 wrapper,3 match-arm helper,5 native-root,16 rerun,1 suppression and4 structural integrations. Selection overlap is not a distinct-test total. See exact argv/subject identities in `candidate-checks.json` and complete logs.

Failures retained: initial worktree doctor behind-main refusal; transient exec-server disconnect (same-executor read-only retry succeeded, no failed batch admission); initial new sparse-name chain static-expectation correction with native truth unchanged; indexed-body guard trial tree8c9aa28ab66caadc24efe3dce6bb98accf5884a8 and patch75b62ef457cfba2da7b28b741310cfb51559a86ec1da35ac837eab2883e5573f, which still produced24 false owner rows; strict opaque-class corpus initializer assertion/outer-batch continuation, preserved in local ancestor86d038ef62e5232bc15bfe0e4ff3da22cd0f5257 and followed by complete corpus commit a5; dirty-vs-clean build byte-equality assertion (build commit stamp differs), followed by a fresh committed executable and150+3 reruns; oversized quarantine admission refusal before archive generation/retirement. These are not erased by subsequent green results.

Baseline a2 check-fast passed15 gates, zero skips; independent141-path selectors agree. Baseline narrow all-targets check passed. Current-head check-fast/precommit, goldens, honesty, dogfood and all-four-corpora are RUNNING in gate driver16684, `/workspace/8004-repair/candidate-gates-driver.log`; no outcome is claimed yet. Hosted/default-feature qualification, Windows and full-workspace runtime qualification remain unestablished here. Known Rust-only uv fixture repairs remain with #8114/#8141; no duplicate repair.

Storage: reserve stayed8GiB. Three smaller owned-witness archives each passed complete separate byte and required-metadata restoration before expanded copies were retired. All201 executable witnesses remain recoverable in the local archives named by `resources/*/verification.json`. The current committed executable and every raw source/log remain expanded. No inherited target or other-owner source was cleaned; no archive purge is authorized. Archive binary bytes are retained locally, not duplicated into this compact Git packet. Restore only through the manifest instructions and verify hashes/metadata.

Reviewer packet b78adbd0ce0dfd5f0baf438497b292133e1c1dc0 was recovered and every manifest entry verified (manifest SHA25664ccc9865e405993c126665bb7c4d7766246737739a2f0845b4aaa12e5b5a445). Prior checkpoint3e2e2347f8734116bc4c79f3b7ea724f43936b98 remains immutable in artifact history. This packet contains raw evidence and historical drivers; canonical runnable commands are the retained argv receipts, not an assumption that historical driver paths work in an arbitrary checkout.
''')
files=[p for p in sorted(o.rglob('*')) if p.is_file() and p.name!='SHA256SUMS']
(o/'SHA256SUMS').write_text(''.join(hashlib.sha256(p.read_bytes()).hexdigest()+'  '+str(p.relative_to(o))+'\n' for p in files))
print(json.dumps(dict(files=len(files)+1,bytes=sum(p.stat().st_size for p in o.rglob('*') if p.is_file()),manifest_sha256=hashlib.sha256((o/'SHA256SUMS').read_bytes()).hexdigest())))
