"""Real-code tasks: published crates already vendored in the local cargo registry.
The base commit is an empty library; the feature commit adds the crate's source (inline
`#[cfg(test)]` modules stripped) plus a thin smoke test. Only src/lib.rs is mutated."""
import glob, os
REAL = {}

def strip_tests(src):
    out, i, n = [], 0, len(src)
    lines = src.split("\n"); k = 0
    while k < len(lines):
        if lines[k].strip().startswith("#[cfg(test)]") and k + 1 < len(lines) and lines[k + 1].lstrip().startswith("mod "):
            depth = 0; started = False
            while k < len(lines):
                depth += lines[k].count("{") - lines[k].count("}")
                if "{" in lines[k]: started = True
                k += 1
                if started and depth <= 0: break
            continue
        out.append(lines[k]); k += 1
    return "\n".join(out)

def real(id, crate_glob, cargo_toml, weak_tests, extra=()):
    d = glob.glob(os.path.expanduser(f"~/.cargo/registry/src/*/{crate_glob}"))
    if not d: return
    d = d[0]
    src = strip_tests(open(f"{d}/src/lib.rs").read())
    extra_files = {f"src/{e}": open(f"{d}/src/{e}").read() for e in extra}
    REAL[id] = dict(id=id, msg=f"feat: add {crate_glob} sources", base_src="", new_src=src, weak_tests=weak_tests,
                    mutants=None, ref_tests="", base_tests="", extra_files=extra_files, cargo_toml=cargo_toml)

real("rstrsim", "strsim-0.11.1", '[package]\nname = "rstrsim"\nversion = "0.1.0"\nedition = "2021"\n[lib]\npath = "src/lib.rs"\ndoctest = false\n',
'''use rstrsim::*;
#[test]
fn hamming_basic() { assert_eq!(hamming("hamming", "hammers"), Ok(3)); }
#[test]
fn levenshtein_basic() { assert_eq!(levenshtein("kitten", "sitting"), 3); }
#[test]
fn jaro_winkler_in_range() { let s = jaro_winkler("martha", "marhta"); assert!(s > 0.9 && s <= 1.0); }
''')
real("rshlex", "shlex-2.0.1", '[package]\nname = "rshlex"\nversion = "0.1.0"\nedition = "2021"\n[features]\ndefault = ["std"]\nstd = []\n[lib]\npath = "src/lib.rs"\ndoctest = false\n',
'''use rshlex::*;
#[test]
fn split_simple() { assert_eq!(split("a b"), Some(vec!["a".to_string(), "b".to_string()])); }
#[test]
fn unterminated_quote_is_none() { assert_eq!(split("a \\"b"), None); }
''', extra=("bytes.rs",))
real("rpercent", "percent-encoding-2.3.2", '[package]\nname = "rpercent"\nversion = "0.1.0"\nedition = "2021"\n[features]\ndefault = ["alloc", "std"]\nalloc = []\nstd = []\n[lib]\npath = "src/lib.rs"\ndoctest = false\n',
'''use rpercent::*;
#[test]
fn encodes_space() { assert_eq!(utf8_percent_encode("a b", NON_ALPHANUMERIC).to_string(), "a%20b"); }
#[test]
fn decodes_space() { assert_eq!(percent_decode_str("a%20b").decode_utf8().unwrap(), "a b"); }
''', extra=("ascii_set.rs",))
