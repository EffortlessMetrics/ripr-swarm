//! Thin CLI adapter for `ripr first-pr` (#4854 extraction).

use crate::output;

pub(in crate::cli) fn first_pr(args: &[String]) -> Result<(), String> {
    output::first_pr::first_pr(args)
}

#[cfg(test)]
mod tests {
    use super::super::tests::args;
    use super::first_pr;

    #[test]
    fn first_pr_dispatch_wiring_help_exits_cleanly() {
        assert_eq!(first_pr(&args(&["--help"])), Ok(()));
    }
}
