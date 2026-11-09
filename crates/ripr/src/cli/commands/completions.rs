//! Dispatch for `ripr completions`.
//!
//! CLI adapter only: the command names come from the governed command catalog
//! (`crate::cli::command_catalog`), so a command added there is completed
//! without a second list to keep in sync. The scripts complete the command name
//! and, for commands that have them, the word after the command. Flags are not completed.

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
            "complete -c ripr -f -n 'test (count (commandline -opc)) -eq 2; and contains -- {top} (commandline -opc)[2]' -a '{}'\n",
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

    fn entry(
        path: &'static str,
        class: CommandClass,
        discovery: DiscoveryPosture,
        relation: CommandRelation,
    ) -> CommandCatalogEntry {
        CommandCatalogEntry {
            id: "cmd:synthetic",
            path,
            aliases: &[],
            class,
            dispatch: crate::cli::command_catalog::CommandDispatch::Help,
            parser_owner: "cli::command",
            dispatch_owner: "cli::execute",
            help_owner: "cli::help",
            discovery,
            relation,
        }
    }

    /// Two public commands (one with two subcommands), plus an alias, an
    /// advanced row and a hidden row that must never be completed.
    fn synthetic() -> Vec<CommandCatalogEntry> {
        let public = |path| {
            entry(
                path,
                CommandClass::Public,
                DiscoveryPosture::OrdinaryPublic,
                CommandRelation::Canonical,
            )
        };
        vec![
            public("zeta"),
            public("alpha"),
            public("alpha two"),
            public("alpha one"),
            entry(
                "oldname",
                CommandClass::Compatibility,
                DiscoveryPosture::CompatibilityAlias,
                CommandRelation::AliasOf("alpha"),
            ),
            entry(
                "expert",
                CommandClass::Advanced,
                DiscoveryPosture::Advanced,
                CommandRelation::Canonical,
            ),
            entry(
                "internal",
                CommandClass::Internal,
                DiscoveryPosture::Hidden,
                CommandRelation::Canonical,
            ),
        ]
    }

    #[test]
    fn tree_keeps_only_public_canonical_rows_in_sorted_order() {
        let tree = command_tree(&synthetic());
        let tops: Vec<_> = tree.keys().copied().collect();
        assert_eq!(tops, ["alpha", "zeta"]);
        let subs: Vec<_> = tree["alpha"].iter().copied().collect();
        assert_eq!(subs, ["one", "two"]);
        assert!(tree["zeta"].is_empty());
    }

    #[test]
    fn bash_output_is_exact_for_a_synthetic_catalog() {
        let expected = "\
# bash completion for ripr. Load it with: source <(ripr completions bash)
_ripr() {
    local cur=\"${COMP_WORDS[COMP_CWORD]}\"
    if [ \"$COMP_CWORD\" -eq 1 ]; then
        COMPREPLY=( $(compgen -W \"alpha zeta\" -- \"$cur\") )
        return
    fi
    if [ \"$COMP_CWORD\" -eq 2 ]; then
        case \"${COMP_WORDS[1]}\" in
            alpha) COMPREPLY=( $(compgen -W \"one two\" -- \"$cur\") ) ;;
        esac
    fi
}
complete -F _ripr ripr
";
        assert_eq!(render("bash", &synthetic()), expected);
    }

    #[test]
    fn zsh_output_is_exact_for_a_synthetic_catalog() {
        let expected = "\
#compdef ripr
# zsh completion for ripr. Load it with: source <(ripr completions zsh)
_ripr() {
    if (( CURRENT == 2 )); then
        compadd -- alpha zeta
    elif (( CURRENT == 3 )); then
        case \"$words[2]\" in
            alpha) compadd -- one two ;;
        esac
    fi
}
if [ \"$funcstack[1]\" = \"_ripr\" ]; then
    _ripr \"$@\"
else
    compdef _ripr ripr
fi
";
        assert_eq!(render("zsh", &synthetic()), expected);
    }

    #[test]
    fn fish_output_is_exact_for_a_synthetic_catalog() {
        let expected = "\
# fish completion for ripr. Load it with: ripr completions fish | source
complete -c ripr -f -n '__fish_use_subcommand' -a 'alpha zeta'
complete -c ripr -f -n 'test (count (commandline -opc)) -eq 2; and contains -- alpha (commandline -opc)[2]' -a 'one two'
";
        assert_eq!(render("fish", &synthetic()), expected);
    }

    #[test]
    fn powershell_output_is_exact_for_a_synthetic_catalog() {
        let expected = "\
# PowerShell completion for ripr. Load it with: ripr completions powershell | Out-String | Invoke-Expression
Register-ArgumentCompleter -Native -CommandName ripr -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)
    $parts = @($commandAst.CommandElements | ForEach-Object { $_.ToString() })
    $done = $parts.Count - 1
    if ($wordToComplete -ne '') { $done = $done - 1 }
    $choices = @()
    if ($done -eq 0) { $choices = @('alpha', 'zeta') }
    elseif ($done -eq 1 -and $parts[1] -eq 'alpha') { $choices = @('one', 'two') }
    $choices | Where-Object { $_ -like \"$wordToComplete*\" } | ForEach-Object {
        [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_)
    }
}
";
        assert_eq!(render("powershell", &synthetic()), expected);
    }

    #[test]
    fn no_shell_completes_an_alias_or_a_non_public_row() {
        for shell in SHELLS {
            let script = render(shell, &synthetic());
            for hidden in ["oldname", "expert", "internal"] {
                assert!(!script.contains(hidden), "{shell} completes {hidden}");
            }
        }
    }

    #[test]
    fn real_catalog_scripts_name_real_commands_and_leave_out_aliases() {
        let tree = command_tree(catalog());
        assert!(tree.contains_key("check"));
        assert!(tree.contains_key("completions"));
        assert!(tree["cache"].contains("export"));
        for entry in catalog() {
            if entry.class == CommandClass::Compatibility {
                assert!(!tree.contains_key(entry.path), "{}", entry.path);
            }
        }
        for shell in SHELLS {
            let script = render(shell, catalog());
            assert!(script.contains("completions"), "{shell}");
        }
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
        assert!(flag.contains("--bogus"), "{flag}");
    }
}
