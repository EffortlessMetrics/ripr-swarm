mod protocol;
mod server;
mod transport;

use std::path::PathBuf;

pub(super) const MAX_MESSAGE_BYTES: usize = 256 * 1024;
pub(super) const MAX_RESPONSE_BYTES: usize = 128 * 1024;

pub(crate) const MCP_HELP: &str = r#"Expose RIPR's bounded, read-only workspace status over the Model Context Protocol.

Usage: ripr mcp [--stdio] [--root PATH]

Options:
  --stdio       Serve newline-delimited MCP JSON-RPC over stdin/stdout. This is
                the default and only transport in the first supported slice.
  --root PATH   Use this exact repository root. Without it, RIPR starts at the
                current directory and walks ancestors to the nearest supported
                repository marker.
  --help, -h    Print this help.
  --version, -V Print the MCP server version.

The MCP surface is read-only. It exposes `ripr_workspace_status` and
`ripr://workspace/status`; it does not edit source, execute verification or
mutation, load project-local provider configuration, or embed a model provider.
Protocol messages are the only stdout output. Operational failures use stderr.
"#;

pub fn run(args: &[String]) -> Result<(), String> {
    let mut explicit_root = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => {
                println!("{MCP_HELP}");
                return Ok(());
            }
            "--version" | "-V" => {
                println!("ripr-mcp {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--stdio" => {}
            "--root" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("missing value for --root".to_string());
                };
                if explicit_root.is_some() {
                    return Err("--root may be passed only once".to_string());
                }
                explicit_root = Some(PathBuf::from(value));
                index += 1;
            }
            argument => {
                return Err(format!(
                    "unknown mcp argument {argument:?}. Run `ripr mcp --help`."
                ));
            }
        }
        index += 1;
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("create MCP runtime: {error}"))?;
    runtime.block_on(transport::serve_stdio(explicit_root))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn parser_rejects_unknown_and_ambiguous_root_arguments() {
        assert_eq!(
            run(&args(&["--bad"])),
            Err("unknown mcp argument \"--bad\". Run `ripr mcp --help`.".to_string())
        );
        assert_eq!(
            run(&args(&["--root"])),
            Err("missing value for --root".to_string())
        );
        assert_eq!(
            run(&args(&["--root", ".", "--root", "."])),
            Err("--root may be passed only once".to_string())
        );
    }
}
