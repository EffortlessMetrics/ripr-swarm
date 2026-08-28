#![forbid(unsafe_code)]

pub(crate) fn run() -> Result<(), String> {
    dispatch(collect_args())
}

fn dispatch(args: Vec<String>) -> Result<(), String> {
    if let Some(mcp_args) = routed_mcp_args(&args) {
        return ripr::mcp::run(&mcp_args);
    }
    ripr::cli::run(args)
}

fn routed_mcp_args(args: &[String]) -> Option<Vec<String>> {
    if args.get(1).is_some_and(|argument| argument == "mcp") {
        return Some(args.iter().skip(2).cloned().collect());
    }
    if args.get(1).is_some_and(|argument| argument == "help")
        && args.get(2).is_some_and(|argument| argument == "mcp")
    {
        let mut routed = vec!["--help".to_string()];
        routed.extend(args.iter().skip(3).cloned());
        return Some(routed);
    }
    None
}

fn collect_args() -> Vec<String> {
    std::env::args().collect()
}

#[cfg(test)]
mod tests {
    use super::routed_mcp_args;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn startup_routes_direct_and_help_mcp_invocations_before_general_cli() {
        assert_eq!(
            routed_mcp_args(&args(&["ripr", "mcp", "--stdio", "--root", "."])),
            Some(args(&["--stdio", "--root", "."]))
        );
        assert_eq!(
            routed_mcp_args(&args(&["ripr", "help", "mcp"])),
            Some(args(&["--help"]))
        );
        assert_eq!(routed_mcp_args(&args(&["ripr", "check"])), None);
    }
}
