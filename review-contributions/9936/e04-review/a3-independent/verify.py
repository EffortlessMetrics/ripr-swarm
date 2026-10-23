import ast, hashlib, json, pathlib, re, subprocess, datetime
R=pathlib.Path('/workspace/review-9936')
D=pathlib.Path('/workspace/9936-a3-review')
HEAD='a3b0e245646a41e183d0db6e7ee3e23f9c683c38'
PARENT='e04f482bb13f559e01f8216294ecd4a9a10acc9c'
PATH='crates/ripr/src/analysis/language/typescript/tests/var_runner_binding_tests.rs'
def git(*args): return subprocess.check_output(['git',*args],cwd=R)
def sha(b): return hashlib.sha256(b).hexdigest()
meta=git('cat-file','-p',HEAD).decode()
assert 'tree 03d0ff4e4d6b3c088865d8df258a3a7373e3b279\nparent '+PARENT+'\n' in meta
assert git('diff','--name-only',PARENT,HEAD).decode().splitlines()==[PATH]
src=git('show',HEAD+':'+PATH).decode()
tape=re.search(r'fn tape_test\(.*?\n\}',src,re.S).group()
start=src.index('fn review_9936_comparison_regex_after_angle_shapes_keeps_the_write_visible()')
part=src[start:]
assembly=part[part.index('    let regex ='):part.index('    for (index,')]
helper=tape+'\nfn main() {\n'+assembly+'''    for (index, (file, tail)) in cases.iter().enumerate() {
        let ext = if file.ends_with("tsx") { "tsx" } else { "ts" };
        let dir = "/workspace/9936-a3-review";
        std::fs::write(format!("{dir}/tail-{index}.{ext}"), tail).unwrap();
        let fixture = tape_test("var test = require('tape');", tail);
        std::fs::write(format!("{dir}/fixture-{index}.{ext}"), &fixture).unwrap();
        let declarations = "export {};\\ndeclare function require(path: string): any;\\ndeclare function Foo(props: any): any;\\ndeclare global { namespace JSX { type Element=any; interface IntrinsicElements { p: any; } } }\\n";
        std::fs::write(format!("{dir}/native-{index}.{ext}"), format!("{declarations}{fixture}")).unwrap();
    }
}\n'''
(D/'fixture-generator.rs').write_text(helper)
rustc='/workspace/.ripr-toolchain/rustup/toolchains/1.95.0-x86_64-unknown-linux-gnu/bin/rustc'
p=subprocess.run([rustc,'--edition','2024',str(D/'fixture-generator.rs'),'-o',str(D/'fixture-generator')],capture_output=True,text=True)
(D/'fixture-generator-build.log').write_text(p.stdout+p.stderr);assert p.returncode==0
subprocess.run([str(D/'fixture-generator')],check=True)
prior=json.load(open('/workspace/9936-canonical-e04/corrected-new-test.json'))
rows=[]
for i,old in enumerate(prior):
    kind=old['kind'];tail=(D/f'tail-{i}.{kind}').read_text();assert tail==old['source']
    fixture=(D/f'fixture-{i}.{kind}').read_text()
    native=['node','/workspace/9936-tools/node_modules/typescript/bin/tsc','--noEmit','--target','ES2022','--skipLibCheck','--jsx','preserve',str(D/f'native-{i}.{kind}')]
    proc=subprocess.run(native,capture_output=True,text=True)
    (D/f'native-{i}.log').write_text(proc.stdout+proc.stderr)
    assert proc.returncode==0,(i,proc.stdout,proc.stderr)
    current=subprocess.run(['/workspace/9936-integration-independent/consumer',fixture,kind],capture_output=True,text=True)
    obsolete=subprocess.run(['/workspace/9936-canonical-bdd0/consumer',fixture],capture_output=True,text=True)
    assert current.returncode==obsolete.returncode==0
    got=ast.literal_eval(current.stdout.splitlines()[-1]);oldgot=ast.literal_eval(obsolete.stdout.splitlines()[-1])
    expected=[m.start() for m in re.finditer('test = null',fixture)]
    assert expected and all(('test',x) in got for x in expected),(i,got,expected)
    assert not any(('test',x) in oldgot for x in expected),(i,oldgot,expected)
    rows.append(dict(index=i,kind=kind,tail_identical_to_prior_validated=True,native_command=native,native_exit=proc.returncode,expected_write_offsets=expected,current_write_offsets=got,bdd0_write_offsets=oldgot,current_raw=current.stdout,bdd0_raw=obsolete.stdout))
(D/'five-controls.json').write_text(json.dumps(rows,indent=2)+'\n')
components=['classifier.rs','literal_mask.rs','tests_extract.rs','admission.rs','mock_payload_guard.rs','mock_payload_guard/spy.rs','mod.rs']
identity=[]
for name in components:
    path='crates/ripr/src/analysis/language/typescript/'+name
    b=git('show',HEAD+':'+path)
    identities={h:git('rev-parse',h+':'+path).decode().strip() for h in [HEAD,PARENT,'017df5f102194b398c9f9879df8becda9efdc45b']}
    assert len(set(identities.values()))==1
    identity.append(dict(path=path,sha256=sha(b),blobs=identities))
(D/'source-identity.json').write_text(json.dumps(identity,indent=2)+'\n')
(D/'committed-diff.patch').write_bytes(git('diff',PARENT,HEAD,'--'))
(D/'commit.txt').write_text(meta)
print('a3 exact test expansion:5/5 identical to independently validated tails; native full fixtures:5/5 exit0; exact normal->byte wrapper->strict scanner finds all actual writes; obsolete bdd0 misses all5; production components byte-identical e04/017df.')
