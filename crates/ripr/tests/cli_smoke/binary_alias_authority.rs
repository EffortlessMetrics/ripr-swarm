use super::{ignore_remove_dir_all, run_git, run_ripr, run_ripr_with_env};

#[test]
fn draft_finds_test_wrapped_by_unchanged_dependency() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!("ripr-dependency-alias-{}", std::process::id()));
    std::fs::create_dir_all(root.join("app/src"))?;
    std::fs::create_dir_all(root.join("macros/src"))?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers=['app','macros']\nresolver='2'\n",
    )?;
    let app = "[package]\nname='app'\nversion='0.1.0'\nedition='2021'\n[dependencies]\nmacros={path='../macros'}\n";
    std::fs::write(root.join("app/Cargo.toml"), app)?;
    let macros_manifest =
        "[package]\nname='macros'\nversion='0.1.0'\nedition='2021'\n[lib]\nproc-macro=true\n";
    std::fs::write(root.join("macros/Cargo.toml"), macros_manifest)?;
    std::fs::write(
        root.join("macros/src/lib.rs"),
        "extern crate proc_macro;\nuse proc_macro::TokenStream;\n#[proc_macro_attribute]\npub fn wrapped_test(_: TokenStream, item: TokenStream) -> TokenStream { let mut result: TokenStream = stringify!(#[test]).parse().unwrap(); result.extend(item); result }\n",
    )?;
    let lib = "use macros::wrapped_test as test;\npub mod handler;\n";
    std::fs::write(root.join("app/src/lib.rs"), lib)?;
    let handler =
        "pub fn value() -> u32 { 1 }\n#[crate::test]\nfn aliased() { assert_eq!(value(), 2); }\n";
    std::fs::write(root.join("app/src/handler.rs"), handler)?;
    run_git(&root, &["init", "-q"])?;
    run_git(&root, &["config", "user.email", "test@example.invalid"])?;
    run_git(&root, &["config", "user.name", "Test"])?;
    run_git(&root, &["add", "."])?;
    run_git(&root, &["commit", "-q", "-m", "base"])?;
    std::fs::write(
        root.join("app/src/handler.rs"),
        handler.replace("u32 { 1 }", "u32 { 2 }"),
    )?;
    let root_arg = root.to_string_lossy().into_owned();
    let check = |mode: &str,
                 base: &str,
                 snapshot: &str,
                 env: &[(&str, &str)]|
     -> Result<bool, Box<dyn std::error::Error>> {
        let output = run_ripr_with_env(
            &[
                "check", "--root", &root_arg, "--base", base, snapshot, "--mode", mode, "--format",
                "json",
            ],
            env,
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        let findings = value["findings"].as_array().ok_or("findings")?;
        assert_eq!(findings.len(), 1);
        let tests = findings[0]["related_tests"]
            .as_array()
            .ok_or("related tests")?;
        Ok(tests.iter().any(|test| test["name"] == "aliased"))
    };
    let worktree = |mode, env: &[(&str, &str)]| check(mode, "HEAD", "--worktree", env);
    assert!(worktree("draft", &[])?);
    assert!(worktree("draft", &[])?); // warm file-fact cache
    assert!(worktree("fast", &[])?);
    assert!(!worktree("draft", &[("RIPR_MAX_DIFF_INDEX_FILES", "2")])?);
    std::fs::write(
        root.join("app/Cargo.toml"),
        app.replace("../macros", "../other"),
    )?;
    assert!(!worktree("draft", &[])?);
    std::fs::write(
        root.join("app/Cargo.toml"),
        format!("{app}\n[lib]\ntest=false\n"),
    )?;
    assert!(!worktree("draft", &[])?);
    std::fs::write(root.join("app/Cargo.toml"), app)?;
    std::fs::write(
        root.join("app/src/lib.rs"),
        lib.replace("wrapped_test", "other_test"),
    )?;
    assert!(!worktree("draft", &[])?);
    std::fs::write(
        root.join("app/src/lib.rs"),
        format!("{lib}\nmod macros {{}}\n"),
    )?;
    assert!(!worktree("draft", &[])?);
    std::fs::write(root.join("app/src/lib.rs"), lib)?;
    run_git(&root, &["add", "."])?;
    run_git(&root, &["commit", "-q", "-m", "change"])?;
    std::fs::write(
        root.join("macros/Cargo.toml"),
        macros_manifest.replace("name='macros'", "name='other'"),
    )?;
    assert!(check("draft", "HEAD~1", "--committed", &[])?);
    assert!(!check("draft", "HEAD~1", "--worktree", &[])?);
    ignore_remove_dir_all(&root);
    Ok(())
}

#[test]
fn binary_alias_manifest_authority_follows_the_selected_snapshot()
-> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!(
        "ripr-binary-alias-authority-{}",
        std::process::id()
    ));
    std::fs::create_dir(&root)?;
    std::fs::create_dir_all(root.join("app/src/bin"))?;
    std::fs::create_dir_all(root.join("macros/src"))?;
    let workspace = "[workspace]\nmembers=['app','macros']\nresolver='2'\n";
    let app = "[package]\nname='app'\nversion='0.1.0'\nedition='2021'\nautobins=true\n[dependencies]\nmy_macros={path='../macros'}\n";
    let macros =
        "[package]\nname='my_macros'\nversion='0.1.0'\nedition='2021'\n[lib]\nproc-macro=true\n";
    let wrapper = "#[proc_macro_attribute]\npub fn wrapped_test(a: TokenStream, b: TokenStream) -> TokenStream { quote! { #[::tokio::test] #b } }\n";
    let worker = "use my_macros::wrapped_test as test;\nfn value() -> u32 { 1 }\n#[crate::test]\nfn aliased() { assert_eq!(value(), 2); }\nfn main() {}\n";
    std::fs::write(root.join("Cargo.toml"), workspace)?;
    std::fs::write(root.join("app/Cargo.toml"), app)?;
    std::fs::write(root.join("macros/Cargo.toml"), macros)?;
    std::fs::write(root.join("macros/src/lib.rs"), wrapper)?;
    std::fs::write(root.join("app/src/bin/worker.rs"), worker)?;
    run_git(&root, &["init", "-q"])?;
    run_git(&root, &["config", "user.email", "test@example.invalid"])?;
    run_git(&root, &["config", "user.name", "Test"])?;
    run_git(&root, &["add", "."])?;
    run_git(&root, &["commit", "-q", "-m", "base"])?;
    std::fs::write(
        root.join("app/src/bin/worker.rs"),
        worker.replace("u32 { 1 }", "u32 { 2 }"),
    )?;
    run_git(&root, &["commit", "-q", "-a", "-m", "change"])?;
    let root_arg = root.to_string_lossy().into_owned();
    let check = |base: &str, snapshot: &str| -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let out = run_ripr(&[
            "check", "--root", &root_arg, "--base", base, snapshot, "--mode", "deep", "--format",
            "json",
        ]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&out.stdout)?;
        let findings = value["findings"].as_array().ok_or("findings")?;
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0]["probe"]["family"], "return_value");
        Ok(findings[0]["related_tests"]
            .as_array()
            .ok_or("related tests")?
            .iter()
            .map(|test| test["name"].as_str().unwrap_or_default().to_owned())
            .collect())
    };
    assert_eq!(check("HEAD~1", "--committed")?, ["aliased"]);
    // The legacy workspace-name inventory reads the worktree. It must not
    // veto a binary alias whose committed provider manifest is unchanged.
    std::fs::write(
        root.join("macros/Cargo.toml"),
        macros.replace("name='my_macros'", "name='other_macros'"),
    )?;
    assert_eq!(check("HEAD~1", "--committed")?, ["aliased"]);
    std::fs::write(root.join("macros/Cargo.toml"), macros)?;
    std::fs::write(
        root.join("app/Cargo.toml"),
        app.replace("autobins=true", "autobins=false")
            .replace("my_macros={path='../macros'}", "my_macros='1'"),
    )?;
    assert_eq!(check("HEAD~1", "--committed")?, ["aliased"]);
    assert!(check("HEAD~1", "--worktree")?.is_empty());

    // Reverse the dirty flag: the worktree can enable a binary that HEAD
    // disables, but it must not invent committed test evidence.
    std::fs::write(
        root.join("app/Cargo.toml"),
        app.replace("autobins=true", "autobins=false"),
    )?;
    run_git(&root, &["commit", "-q", "-a", "-m", "disable"])?;
    std::fs::write(root.join("app/Cargo.toml"), app)?;
    assert!(check("HEAD~2", "--committed")?.is_empty());
    assert_eq!(check("HEAD~2", "--worktree")?, ["aliased"]);
    ignore_remove_dir_all(&root);
    Ok(())
}
