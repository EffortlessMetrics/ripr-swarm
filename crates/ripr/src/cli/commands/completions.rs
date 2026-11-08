//! Dispatch for `ripr completions`.
//!
//! CLI adapter only: the command names come from the governed command catalog
//! (`crate::cli::command_catalog`), so a command added there is completed
//! without a second list to keep in sync. The scripts complete the command name
//! and, for commands that have them, the subcommand. Flags are not completed.

use std::collections::{BTreeMap, BTreeSet};

use crate::cli::command_catalog::{
    CommandCatalogEntry, CommandClass, CommandRelation, DiscoveryPosture, catalog,
};
use crate::cli::help;
use crate::cli::suggest::{unknown_argument, unknown_value};

const SHELLS: &[&str] = &["bash", "zsh", "fish", "powershell"];

type CommandTree = BTreeMap<&'static str, BTreeSet<&'static str>>;

pub(in crate::cli) fn completions(args: &[String]) -> Result<(), String> {
    let mut shell: Option<&str> = None;
    for arg in args {
        match arg.as_str() {
            "--help" | "-h" => {
                help::print_completions_help();
                return Ok(());
            }
            other if other.starts_with('-') => return Err(unknown_argument("completions", other)),
            other => {
                if shell.is_some() {
                    return Err(
                        "completions takes one shell name. Run `ripr completions --help`."
                            .to_string(),
                    );
                }
                shell = Some(other);
            }
        }
    }
    let Some(shell) = shell else {
        return Err(format!(
            "completions needs a shell name: {}. Run `ripr completions --help`.",
            SHELLS.join(", ")
        ));
    };
    if !SHELLS.contains(&shell) {
        return Err(unknown_value("completions shell", shell, SHELLS));
    }
    print!("{}", render(shell, catalog()));
    Ok(())
}

/// Top-level names and their subcommands, from ordinary public catalog rows.
fn command_tree(entries: &[CommandCatalogEntry]) -> CommandTree {
    let mut tree = CommandTree::new();
    for entry in entries {
        if entry.class != CommandClass::Public
            || entry.discovery != DiscoveryPosture::OrdinaryPublic
            || !matches!(entry.relation, CommandRelation::Canonical)
        {
            continue;
        }
        let mut parts = entry.path.split(' ');
        let Some(top) = parts.next() else { continue };
        let subs = tree.entry(top).or_default();
        if let Some(sub) = parts.next() {
            subs.insert(sub);
        }
    }
    tree
}

fn render(shell: &str, entries: &[CommandCatalogEntry]) -> String {
    let tree = command_tree(entries);
    match shell {
        "bash" => render_bash(&tree),
        "zsh" => render_zsh(&tree),
        "fish" => render_fish(&tree),
        _ => render_powershell(&tree),
    }
}

fn spaced<'a>(items: impl IntoIterator<Item = &'a &'static str>) -> String {
    items.into_iter().copied().collect::<Vec<_>>().join(" ")
}

fn quoted<'a>(items: impl IntoIterator<Item = &'a &'static str>) -> String {
    items
        .into_iter()
        .map(|item| format!("'{item}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn with_subs(tree: &CommandTree) -> impl Iterator<Item = (&&'static str, &BTreeSet<&'static str>)> {
    tree.iter().filter(|(_, subs)| !subs.is_empty())
}

fn render_bash(tree: &CommandTree) -> String {
    let mut out = String::from(
        "# bash completion for ripr. Load it with: source <(ripr completions bash)\n_ripr() {\n    local cur=\"${COMP_WORDS[COMP_CWORD]}\"\n",
    );
    out.push_str(&format!(
        "    if [ \"$COMP_CWORD\" -eq 1 ]; then\n        COMPREPLY=( $(compgen -W \"{}\" -- \"$cur\") )\n        return\n    fi\n",
        spaced(tree.keys())
    ));
    out.push_str("    if [ \"$COMP_CWORD\" -eq 2 ]; then\n        case \"${COMP_WORDS[1]}\" in\n");
    for (top, subs) in with_subs(tree) {
        out.push_str(&format!(
            "            {top}) COMPREPLY=( $(compgen -W \"{}\" -- \"$cur\") ) ;;\n",
            spaced(subs)
        ));
    }
    out.push_str("        esac\n    fi\n}\ncomplete -F _ripr ripr\n");
    out
}

fn render_zsh(tree: &CommandTree) -> String {
    let mut out = String::from(
        "#compdef ripr\n# zsh completion for ripr. Load it with: source <(ripr completions zsh)\n_ripr() {\n",
    );
    out.push_str(&format!(
        "    if (( CURRENT == 2 )); then\n        compadd -- {}\n    elif (( CURRENT == 3 )); then\n        case \"$words[2]\" in\n",
        spaced(tree.keys())
    ));
    for (top, subs) in with_subs(tree) {
        out.push_str(&format!(
            "            {top}) compadd -- {} ;;\n",
            spaced(subs)
        ));
    }
    out.push_str(
        "        esac\n    fi\n}\nif [ \"$funcstack[1]\" = \"_ripr\" ]; then\n    _ripr \"$@\"\nelse\n    compdef _ripr ripr\nfi\n",
    );
    out
}

fn render_fish(tree: &CommandTree) -> String {
    let mut out =
        String::from("# fish completion for ripr. Load it with: ripr completions fish | source\n");
    out.push_str(&format!(
        "complete -c ripr -f -n '__fish_use_subcommand' -a '{}'\n",
        spaced(tree.keys())
    ));
    for (top, subs) in with_subs(tree) {
        out.push_str(&format!(
            "complete -c ripr -f -n '__fish_seen_subcommand_from {top}' -a '{}'\n",
            spaced(subs)
        ));
    }
    out
}

fn render_powershell(tree: &CommandTree) -> String {
    let mut out = String::from(
        "# PowerShell completion for ripr. Load it with: ripr completions powershell | Out-String | Invoke-Expression\n",
    );
    out.push_str("Register-ArgumentCompleter -Native -CommandName ripr -ScriptBlock {\n");
    out.push_str("    param($wordToComplete, $commandAst, $cursorPosition)\n");
    out.push_str(
        "    $parts = @($commandAst.CommandElements | ForEach-Object { $_.ToString() })\n",
    );
    out.push_str(
        "    $done = $parts.Count - 1\n    if ($wordToComplete -ne '') { $done = $done - 1 }\n",
    );
    out.push_str(&format!(
        "    $choices = @()\n    if ($done -eq 0) {{ $choices = @({}) }}\n",
        quoted(tree.keys())
    ));
    for (top, subs) in with_subs(tree) {
        out.push_str(&format!(
            "    elseif ($done -eq 1 -and $parts[1] -eq '{top}') {{ $choices = @({}) }}\n",
            quoted(subs)
        ));
    }
    out.push_str(
        "    $choices | Where-Object { $_ -like \"$wordToComplete*\" } | ForEach-Object {\n        [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_)\n    }\n}\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Result<(), String> {
        let owned: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
        completions(&owned)
    }

    #[test]
    fn every_shell_script_names_catalog_commands_and_subcommands() {
        for shell in SHELLS {
            let script = render(shell, catalog());
            for command in ["check", "doctor", "completions", "cache", "export"] {
                assert!(script.contains(command), "{shell} omits {command}");
            }
        }
    }

    #[test]
    fn scripts_leave_out_compatibility_aliases() {
        let tree = command_tree(catalog());
        let mut seen_alias = false;
        for entry in catalog() {
            if entry.class == CommandClass::Compatibility {
                seen_alias = true;
                assert!(!tree.contains_key(entry.path), "{}", entry.path);
            }
        }
        assert!(seen_alias, "fixture expects at least one compatibility row");
        assert!(tree.contains_key("check"));
        assert!(tree["cache"].contains("status"));
    }

    #[test]
    fn bash_script_has_the_expected_shape() {
        let script = render("bash", catalog());
        assert!(script.contains("complete -F _ripr ripr"));
        assert!(script.contains("cache) COMPREPLY=("));
    }

    #[test]
    fn arguments_are_checked() {
        assert_eq!(run(&["--help"]), Ok(()));
        assert_eq!(run(&["bash"]), Ok(()));
        let none = run(&[]).err().unwrap_or_default();
        assert!(none.contains("needs a shell name"), "{none}");
        let bad = run(&["tcsh"]).err().unwrap_or_default();
        assert!(bad.contains("tcsh") && bad.contains("bash"), "{bad}");
        let two = run(&["bash", "zsh"]).err().unwrap_or_default();
        assert!(two.contains("one shell"), "{two}");
        let flag = run(&["--bogus"]).err().unwrap_or_default();
        assert!(flag.contains("completions"), "{flag}");
    }
}
