pub(crate) fn check_executable_files() -> Result<(), String> {
    repo_policy::executable_files::check()
}
