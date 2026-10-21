use std::process::Command;

fn compile_probe() -> bool {
    Command::new("rustc")
        .arg("--crate-type=lib")
        .arg("--emit=metadata")
        .arg("--cap-lints=allow")
        .status()
        .is_ok()
}

fn main() {
    let _ = compile_probe();
}
