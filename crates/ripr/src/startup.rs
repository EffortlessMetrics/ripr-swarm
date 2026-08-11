#![forbid(unsafe_code)]

pub(crate) fn run() -> Result<(), String> {
    let mut args = collect_args();
    let version_requested = args
        .iter()
        .skip(1)
        .take_while(|arg| arg.starts_with('-'))
        .any(|arg| matches!(arg.as_str(), "--version" | "-V"));
    // #2610: extract --verbose before command dispatch so it works with any
    // subcommand. The flag is consumed and not passed to the command parser.
    // Version is a side-effect-free identity query, so it must not emit the
    // verbose diagnostic even when callers append or prepend `--verbose`.
    if !version_requested && let Some(pos) = args.iter().position(|a| a == "--verbose" || a == "-v")
    {
        args.remove(pos);
        ripr::set_verbose(true);
        eprintln!("ripr: verbose mode enabled");
    }
    ripr::cli::run(args)
}

fn collect_args() -> Vec<String> {
    std::env::args().collect()
}
