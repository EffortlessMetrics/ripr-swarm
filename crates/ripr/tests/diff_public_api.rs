//! External import and signature guard for the established diff-loader API (#4859).
//! Compilation is the discriminator; this test does not launch Git.

use ripr::analysis::diff::{load_diff_with_effective_base, load_worktree_diff_with_effective_base};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[test]
fn public_effective_base_loaders_keep_string_error_signatures() {
    let _: fn(&Path, Option<&str>, Option<&PathBuf>, Option<Duration>) -> Result<_, String> =
        load_diff_with_effective_base;
    let _: fn(&Path, Option<&str>, Option<Duration>) -> Result<_, String> =
        load_worktree_diff_with_effective_base;
}
