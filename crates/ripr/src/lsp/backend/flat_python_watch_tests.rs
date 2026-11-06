//! RIPR-SPEC-0028: closed root files must reach the config reload path.
use super::*;
#[cfg(all(feature = "lang-python", feature = "lang-rust"))]
use crate::domain::LanguageId;
#[cfg(all(feature = "lang-python", feature = "lang-rust"))]
use futures::StreamExt;

/// Independent coverage oracle for the literal/single-star root patterns.
/// An unsupported pattern is a test error, never silently treated as a match.
fn watches_path(watchers: &[LSPAny], root: &Path, path: &Path) -> Result<bool, String> {
    for watcher in watchers {
        let glob = watcher.get("globPattern").ok_or("missing globPattern")?;
        let (pattern, candidate) = if let Some(pattern) = glob.as_str() {
            (pattern, crate::output::path::path_display_lossy(path))
        } else {
            let base = glob["baseUri"].as_str().ok_or("missing baseUri")?;
            if base != file_uri_for_path(root)?.as_str() {
                continue;
            }
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            (
                glob["pattern"].as_str().ok_or("missing pattern")?,
                crate::output::path::path_display_lossy(relative),
            )
        };
        let matched = if let Some((prefix, suffix)) = pattern.split_once('*') {
            if suffix.contains('*') {
                return Err(format!("unexpected recursive root pattern: {pattern}"));
            }
            candidate
                .strip_prefix(prefix)
                .and_then(|middle| middle.strip_suffix(suffix))
                .is_some_and(|middle| !middle.contains('/'))
        } else {
            candidate == pattern
        };
        if matched {
            return Ok(true);
        }
    }
    Ok(false)
}

#[test]
fn flat_python_root_watchers_cover_only_direct_root_paths() -> Result<(), String> {
    let root = std::env::temp_dir().join("ripr-flat-python-watch-root");
    for relative in [true, false] {
        let watchers = diagnostics_input_watchers(&root, relative)?;
        assert!(watches_path(&watchers, &root, &root.join("app.py"))?);
        for excluded in [
            root.join("lib/app.py"),
            root.join("app.rs"),
            root.with_file_name("other").join("app.py"),
        ] {
            assert!(!watches_path(&watchers, &root, &excluded)?, "{excluded:?}");
        }
    }
    Ok(())
}

#[cfg(all(feature = "lang-python", feature = "lang-rust"))]
fn assert_languages(backend: &Backend, python: bool) -> Result<(), String> {
    let config = backend.analysis_config().ok_or("missing analysis config")?;
    let expected = if python {
        vec![LanguageId::Rust, LanguageId::Python]
    } else {
        vec![LanguageId::Rust]
    };
    assert_eq!(config.repo_config().languages().enabled(), expected);
    assert!(backend.configuration_failure().is_none());
    Ok(())
}

#[cfg(all(feature = "lang-python", feature = "lang-rust"))]
async fn deliver(
    backend: &Backend,
    watchers: &[LSPAny],
    root: &Path,
    changes: &[(&str, FileChangeType)],
    reload: bool,
) -> Result<(), String> {
    let mut events = Vec::new();
    for (name, typ) in changes {
        let path = root.join(name);
        assert!(
            watches_path(watchers, root, &path)?,
            "client must deliver {path:?}"
        );
        events.push(FileEvent {
            uri: file_uri_for_path(&path)?,
            typ: *typ,
        });
    }
    assert_eq!(
        backend.watched_file_change_kinds(&events),
        WatchedFileChanges {
            config_changed: reload,
            ..WatchedFileChanges::default()
        }
    );
    backend
        .did_change_watched_files(DidChangeWatchedFilesParams { changes: events })
        .await;
    Ok(())
}

#[cfg(all(feature = "lang-python", feature = "lang-rust"))]
#[tokio::test(flavor = "current_thread")]
async fn flat_python_registered_events_reload_defaults_and_keep_explicit_config()
-> Result<(), String> {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        for explicit in [false, true] {
            for relative in [false, true] {
                let scratch = tempfile::tempdir().map_err(|e| e.to_string())?;
                let root = scratch.path();
                std::fs::create_dir(root.join("src")).map_err(|e| e.to_string())?;
                std::fs::write(root.join("Cargo.toml"), "[workspace]\n[package]\nname = \"flat_watch\"\nversion = \"0.1.0\"\nedition = \"2024\"\n").map_err(|e| e.to_string())?;
                std::fs::write(root.join("src/lib.rs"), "pub fn f() {}\n").map_err(|e| e.to_string())?;
                crate::testing::fixture_git::fixture_git_ok(root, &["init", "-q"])?;
                if explicit {
                    std::fs::write(root.join("ripr.toml"), "[languages]\nenabled = [\"rust\"]\n").map_err(|e| e.to_string())?;
                }
                let backend_root = root.to_path_buf();
                let (service, socket) = tower_lsp_server::LspService::new(move |client| Backend::new(client, backend_root.clone()));
                let drain = tokio::spawn(async move {
                    let mut socket = Box::pin(socket);
                    while socket.next().await.is_some() {}
                });
                let backend = service.inner();
                let params: InitializeParams = serde_json::from_value(serde_json::json!({
                    "processId": null, "rootUri": file_uri_for_path(root)?.as_str(), "capabilities": {},
                    "initializationOptions": {"baseRef": "ripr-flat-watch-missing-base"}
                })).map_err(|e| e.to_string())?;
                backend.initialize(params).await.map_err(|e| e.to_string())?;
                let watchers = diagnostics_input_watchers(root, relative)?;
                assert_languages(backend, false)?;

                std::fs::write(root.join("app.py"), "def f():\n    return 1\n").map_err(|e| e.to_string())?;
                assert_languages(backend, false)?; // no event yet
                deliver(backend, &watchers, root, &[("app.py", FileChangeType::CREATED)], true).await?;
                assert_languages(backend, !explicit)?;

                std::fs::remove_file(root.join("app.py")).map_err(|e| e.to_string())?;
                assert_languages(backend, !explicit)?; // deletion is not observed until delivery
                deliver(backend, &watchers, root, &[("app.py", FileChangeType::DELETED)], true).await?;
                assert_languages(backend, false)?;

                std::fs::write(root.join("app.py"), "def f():\n    return 1\n").map_err(|e| e.to_string())?;
                deliver(backend, &watchers, root, &[("app.py", FileChangeType::CREATED)], true).await?;
                assert_languages(backend, !explicit)?;
                std::fs::rename(root.join("app.py"), root.join("generated_app.py")).map_err(|e| e.to_string())?;
                assert_languages(backend, !explicit)?;
                deliver(backend, &watchers, root, &[("app.py", FileChangeType::DELETED), ("generated_app.py", FileChangeType::CREATED)], true).await?;
                assert_languages(backend, false)?;
                deliver(backend, &watchers, root, &[("generated_app.py", FileChangeType::CHANGED)], false).await?;
                assert_languages(backend, false)?;

                std::fs::create_dir(root.join("directory.py")).map_err(|e| e.to_string())?;
                deliver(backend, &watchers, root, &[("directory.py", FileChangeType::CREATED)], true).await?;
                assert_languages(backend, false)?;
                #[cfg(unix)] {
                    std::fs::write(root.join("README"), "target").map_err(|e| e.to_string())?;
                    std::os::unix::fs::symlink(root.join("README"), root.join("linked.py")).map_err(|e| e.to_string())?;
                    deliver(backend, &watchers, root, &[("linked.py", FileChangeType::CREATED)], true).await?;
                    assert_languages(backend, false)?;
                }
                backend.invalidate_workspace_root_for_test().await;
                drop(service);
                drain.abort();
                let _ = drain.await;
            }
        }
        Ok(())
    }).await.map_err(|_| "root Python watcher reload timed out".to_string())?
}
