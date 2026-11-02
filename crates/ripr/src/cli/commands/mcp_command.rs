//! CLI adapter for `ripr mcp` (#5193): argv, help, and process interrupts
//! stay in the CLI layer; MCP modules serve the protocol only.

use std::path::PathBuf;

use crate::cli::{help, unknown_argument};
use crate::interrupt;
use crate::mcp::{self, transport};

pub(super) fn mcp(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h")
        && args.iter().any(|arg| arg == "--all")
    {
        return Err(
            "`ripr mcp --help --all` is not supported; extended help is available via \
             `ripr help --all` (or `ripr check --help --all` for check)"
                .to_string(),
        );
    }
    let mut explicit_root = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => {
                println!("{}", help::render_command_help(mcp::MCP_HELP, &[]));
                return Ok(());
            }
            "--version" | "-V" => {
                println!("ripr-mcp {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--stdio" => {}
            "--root" => {
                let value = match args.get(index + 1) {
                    Some(value) if !value.starts_with('-') => value.clone(),
                    Some(unexpected) => {
                        return Err(format!(
                            "missing value for --root; found option {unexpected:?}; pass the workspace directory after --root"
                        ));
                    }
                    None => return Err("missing value for --root; pass the workspace directory after --root (`ripr mcp --help` shows usage)".to_string()),
                };
                if explicit_root.is_some() {
                    return Err(
                        "--root may be passed only once; remove the extra --root".to_string()
                    );
                }
                explicit_root = Some(PathBuf::from(value));
                index += 1;
            }
            argument => {
                return Err(unknown_argument("mcp", argument));
            }
        }
        index += 1;
    }

    mcp::write_terminal_hint(
        std::io::IsTerminal::is_terminal(&std::io::stdin()),
        &mut std::io::stderr().lock(),
    );

    interrupt::install_server_interrupts()
        .map_err(|error| format!("install MCP interrupt containment: {error}"))?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("create MCP runtime: {error}"))?;
    let result = runtime.block_on(transport::serve_stdio(explicit_root));
    runtime.shutdown_background();
    result
}

#[cfg(test)]
mod tests {
    use super::mcp;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn extended_help_pair_is_rejected_on_the_direct_route() -> Result<(), String> {
        for argv in [
            ["--help", "--all"].as_slice(),
            ["--all", "--help"].as_slice(),
            ["-h", "--all"].as_slice(),
        ] {
            let err = mcp(&args(argv))
                .err()
                .ok_or_else(|| format!("{argv:?} must be rejected on the direct MCP route"))?;
            assert!(err.contains("not supported"), "{err}");
            assert!(err.contains("ripr help --all"), "{err}");
        }
        Ok(())
    }

    #[test]
    fn parser_rejects_unknown_and_ambiguous_root_arguments() {
        assert_eq!(
            mcp(&args(&["--bad"])),
            Err("unknown mcp argument \"--bad\". Run `ripr mcp --help`.".to_string())
        );
        assert_eq!(
            mcp(&args(&["--stido"])),
            Err(
                "unknown mcp argument \"--stido\". Did you mean `--stdio`? Run `ripr mcp --help`."
                    .to_string()
            )
        );
        assert_eq!(
            mcp(&args(&["--root"])),
            Err("missing value for --root; pass the workspace directory after --root (`ripr mcp --help` shows usage)".to_string())
        );
        assert!(
            mcp(&args(&["--root", "--help"]))
                .err()
                .is_some_and(|err| err.contains("missing value for --root"))
        );
        assert_eq!(
            mcp(&args(&["--root", ".", "--root", "."])),
            Err("--root may be passed only once; remove the extra --root".to_string())
        );
    }
}
