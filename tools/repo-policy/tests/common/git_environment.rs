//! Fixture commands inherit toolchain inputs, but never an outer Git repository.

pub fn isolate_git_environment(command: &mut std::process::Command) {
    for (name, _) in std::env::vars_os() {
        // Environment names are case-insensitive on Windows. Clearing the
        // whole Git namespace also covers indexed GIT_CONFIG_KEY/ VALUE keys.
        if name
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("GIT_")
        {
            command.env_remove(name);
        }
    }
}
