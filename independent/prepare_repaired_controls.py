import pathlib
lab=pathlib.Path('/workspace/5500-review'); base=(lab/'positive/src/lib.rs').read_text()
cases={
'macro_scrutinee':base.replace('fn charge(', 'macro_rules! early { ($e:expr) => {{ return 600; $e }} }\nfn charge(').replace('match lease.elapsed', 'match early!(lease.elapsed)'),
'arm_return':base.replace('None => 600', 'None => return 600'),
'comment_string_words':base.replace('None => 600','None => { let _ = "return ? break continue yield"; 600 }'),
}
for name in ['some-zero','none-zero','both-zero']:
    cases['scrutinee_'+name]=(lab/('scrutinee-'+name+'.rs')).read_text()
for name,source in cases.items():
    path=lab/name; (path/'src').mkdir(parents=True,exist_ok=True)
    (path/'Cargo.toml').write_text('[package]\nname="review_control"\nversion="0.0.0"\nedition="2024"\n[workspace]\n')
    (path/'src/lib.rs').write_text(source)
    (path/'input.diff').write_text('--- /dev/null\n+++ b/src/lib.rs\n@@ -0,0 +1,%d @@\n'%len(source.splitlines())+''.join('+'+line+'\n' for line in source.splitlines()))
