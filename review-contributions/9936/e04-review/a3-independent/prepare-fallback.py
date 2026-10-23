import pathlib, subprocess, json, hashlib
D=pathlib.Path('/workspace/9936-a3-review')
R=pathlib.Path('/workspace/review-9936')
head='a3b0e245646a41e183d0db6e7ee3e23f9c683c38'
literal=subprocess.check_output(['git','show',head+':crates/ripr/src/analysis/language/typescript/literal_mask.rs'],cwd=R)
D.joinpath('literal_mask.rs').write_bytes(literal)
src=pathlib.Path('/workspace/9936-integration-independent/consumer.rs').read_text()
main=r'''fn main() {
    let cases = [
        ("module", "ts", "const a = /é`/; test = null;", true),
        ("function-body", "ts", "return /é`/; test = null;", true),
        ("class-member", "ts", "run() { return /é`/; }", true),
        ("malformed-typed", "ts", "const café = `é", false),
        ("malformed-unknown", "default", "const café = `é", false),
        ("ambiguous-unknown", "default", "const v = <div>x</div>; test = null; const y = a /g;", false),
        ("ambiguous-ts", "ts", "const v = <div>x</div>; test = null; const y = a /g;", true),
        ("ambiguous-tsx", "tsx", "const v = <div>x</div>; test = null; const y = a /g;", true),
    ];
    for (name, kind, text, expected_known) in cases {
        let source_type = match kind { "ts"=>Some(SourceType::ts()), "tsx"=>Some(SourceType::tsx()), _=>None };
        let prepared = literal_mask::prepare(text, source_type);
        assert_eq!(prepared.regex_known, expected_known, "{name}");
        let legacy = ts_mask_lexically(text, true, &std::collections::BTreeSet::new());
        let normal = if let Some(st) = source_type { ts_mask_literals_and_comments_for_source_type(text, st) } else { ts_mask_literals_and_comments(text) };
        let byte_mask = preserve_mask_byte_width(text, &normal).unwrap();
        assert_eq!(text.len(), byte_mask.len(), "{name}");
        if !expected_known {
            assert_eq!(prepared.code, text, "{name}");
            assert!(prepared.operand_ends.is_empty(), "{name}");
            assert_eq!(normal, legacy, "{name}");
            assert_eq!(byte_mask, preserve_mask_byte_width(text, &legacy).unwrap(), "{name}");
        }
        println!("{name}: regex_known={} unchanged={} operand_ends={:?} byte_width={} legacy_equal={} mask={:?}", prepared.regex_known, prepared.code==text, prepared.operand_ends, byte_mask.len(), normal==legacy, byte_mask);
        for (index,(prefix,suffix)) in [("",""),("async function* __ripr_mask_body__() {\n","\n}"),("class __RiprMaskOwner__ {\n","\n}")].into_iter().enumerate() {
            let code = format!("{prefix}{text}{suffix}");
            let counts: Vec<_> = [SourceType::ts(),SourceType::tsx()].into_iter().map(|st|{let alloc=Allocator::default();Parser::new(&alloc,&code,st).parse().errors.len()}).collect();
            if name.starts_with("malformed") { assert!(counts.iter().all(|n|*n>0), "{name} prefix {index}"); }
            println!("  prefix={index} TS_errors={} TSX_errors={}",counts[0],counts[1]);
        }
    }
}
'''
src=src[:src.rindex('fn main()')]+main
D.joinpath('prepare-fallback.rs').write_text(src)
rustc='/workspace/.ripr-toolchain/rustup/toolchains/1.95.0-x86_64-unknown-linux-gnu/bin/rustc'
deps=pathlib.Path('/workspace/ripr-swarm/target/debug/deps')
cmd=[rustc,'--edition','2024','-A','dead_code',str(D/'prepare-fallback.rs'),'-L',f'dependency={deps}']
for name,fn in [('oxc_allocator','liboxc_allocator-63c70161c767fb6e.rlib'),('oxc_ast','liboxc_ast-8c51c2d567d7d518.rlib'),('oxc_parser','liboxc_parser-37443d22c8e823bf.rlib'),('oxc_span','liboxc_span-7ad3a525337da68b.rlib')]: cmd+=['--extern',f'{name}={deps/fn}']
cmd+=['--extern','oxc_ast_visit=/workspace/9936-repair/liboxc_ast_visit.rlib','-o',str(D/'prepare-fallback')]
D.joinpath('prepare-fallback-command.json').write_text(json.dumps(cmd,indent=2)+'\n')
p=subprocess.run(cmd,capture_output=True,text=True)
D.joinpath('prepare-fallback-build.log').write_text(p.stdout+p.stderr)
assert p.returncode==0,p.stderr
p=subprocess.run([str(D/'prepare-fallback')],capture_output=True,text=True)
D.joinpath('prepare-fallback.log').write_text(p.stdout+p.stderr)
assert p.returncode==0,p.stdout+p.stderr
print(p.stdout)
