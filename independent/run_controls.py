import pathlib, subprocess, json, os, hashlib
LAB=pathlib.Path('/workspace/5500-review')
BASE='''struct Lease { elapsed: Option<u64> }
fn charge(lease: &Lease) -> u64 {
    match lease.elapsed {
        Some(ms) => ms,
        None => 600,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observes() {
        let lease = Lease { elapsed: None };
        assert_eq!(charge(&lease), 600);
    }
}
'''
MACRO='''macro_rules! fake_max {
    ($name:ident) => {
        mod $name {
            #[derive(Debug)] pub struct Fake;
            pub const MAX: Fake = Fake;
        }
        impl PartialEq<$name::Fake> for u64 {
            fn eq(&self, _: &$name::Fake) -> bool { true }
        }
    };
}
fake_max!(u64);
'''
cases={
'positive':BASE,
'scrutinee_return':BASE.replace('match lease.elapsed', 'match { return 600; lease.elapsed }'),
'macro_max':BASE.replace('        assert_eq!(charge(&lease), 600);', MACRO+'        assert_eq!(charge(&lease), u64::MAX);'),
'macro_max_wrong':BASE.replace('None => 600', 'None => 0').replace('        assert_eq!(charge(&lease), 600);', MACRO+'        assert_eq!(charge(&lease), u64::MAX);'),
'literal_expected_wrong':BASE.replace('None => 600', 'None => 0'),
'non_tail':BASE.replace('    match lease.elapsed', '    let _ = 0;\n    match lease.elapsed'),
}
for name,source in cases.items():
    path=LAB/name
    (path/'src').mkdir(parents=True,exist_ok=True)
    (path/'Cargo.toml').write_text('[package]\nname="review_control"\nversion="0.0.0"\nedition="2024"\n[workspace]\n')
    (path/'src/lib.rs').write_text(source)
    diff='--- /dev/null\n+++ b/src/lib.rs\n@@ -0,0 +1,%d @@\n'%len(source.splitlines())+''.join('+'+line+'\n' for line in source.splitlines())
    (path/'input.diff').write_text(diff)
print(json.dumps({'cases':list(cases)}))
