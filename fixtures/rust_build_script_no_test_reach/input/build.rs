fn lint_cap() -> &'static str {
    "--cap-lints=allow"
}

fn main() {
    println!("cargo:rustc-env=LINT_CAP={}", lint_cap());
}
