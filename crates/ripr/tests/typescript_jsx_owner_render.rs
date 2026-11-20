//! CLI regression for #15295: a JSX render `<Owner .../>` in a `.tsx` test is
//! a call of the component, so it reaches the changed owner. The render alone
//! never credits oracle strength (nor does an unrelated assertion beside it), a
//! same-named component from another module is not the owner, a render nested
//! inside another element still reaches, and an unrendered owner stays unreached.

#![cfg(feature = "lang-typescript")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static WORKSPACE_COUNTER: AtomicU64 = AtomicU64::new(0);

struct WorkspaceGuard {
    root: PathBuf,
}

impl Drop for WorkspaceGuard {
    fn drop(&mut self) {
        // Keep the workspace of a failing test for inspection.
        if !std::thread::panicking() {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

fn write_file(root: &Path, relative: &str, body: &str) -> Result<(), String> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("mkdir {}: {error}", parent.display()))?;
    }
    fs::write(&path, body).map_err(|error| format!("write {}: {error}", path.display()))
}

const BADGE: &str = "import React from 'react';\nexport function Badge({ count, max }: { count: number; max: number }) {\n  const label = count > max ? `${max}+` : String(count);\n  return <span className=\"badge\">{label}</span>;\n}\n";
const OTHER_BADGE: &str =
    "import React from 'react';\nexport function Badge() {\n  return <span>other</span>;\n}\n";
const DIFF: &str = "diff --git a/src/Badge.tsx b/src/Badge.tsx\n--- a/src/Badge.tsx\n+++ b/src/Badge.tsx\n@@ -1,5 +1,5 @@\n import React from 'react';\n export function Badge({ count, max }: { count: number; max: number }) {\n-  const label = count >= max ? `${max}+` : String(count);\n+  const label = count > max ? `${max}+` : String(count);\n   return <span className=\"badge\">{label}</span>;\n }\n";

fn workspace(test_body: &str) -> Result<WorkspaceGuard, String> {
    let call = WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let root =
        std::env::temp_dir().join(format!("ripr-ts-jsx-owner-{}-{call}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).map_err(|error| format!("mkdir {}: {error}", root.display()))?;
    let guard = WorkspaceGuard { root };
    let root = &guard.root;
    write_file(
        root,
        "ripr.toml",
        "[languages]\nenabled = [\"typescript\"]\n",
    )?;
    write_file(
        root,
        "package.json",
        "{\"name\":\"badge\",\"devDependencies\":{\"vitest\":\"^1.0.0\"},\"scripts\":{\"test\":\"vitest\"}}\n",
    )?;
    write_file(root, "src/Badge.tsx", BADGE)?;
    write_file(root, "other/Badge.tsx", OTHER_BADGE)?;
    write_file(root, "src/Badge.test.tsx", test_body)?;
    write_file(root, "change.diff", DIFF)?;
    Ok(guard)
}

fn check(root: &Path, format: &str) -> Result<String, String> {
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["check", "--quiet", "--format", format, "--root"])
        .arg(root)
        .arg("--diff")
        .arg(root.join("change.diff"))
        .output()
        .map_err(|error| format!("spawn ripr check: {error}"))?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn finding(root: &Path) -> Result<serde_json::Value, String> {
    let json: serde_json::Value =
        serde_json::from_str(&check(root, "json")?).map_err(|error| error.to_string())?;
    json.pointer("/findings/0")
        .cloned()
        .ok_or_else(|| format!("no finding: {json}"))
}

const RENDERED: &str = "import React from 'react';\nimport { render, screen } from '@testing-library/react';\nimport { it, expect } from 'vitest';\nimport { Badge } from './Badge';\nit('caps', () => { render(<Badge count={120} max={99} />); expect(screen.getByText('99+').textContent).toBe('99+'); });\n";

#[test]
fn a_rendered_owner_is_reached_and_not_told_to_call_it() -> Result<(), String> {
    let guard = workspace(RENDERED)?;
    let human = check(&guard.root, "human")?;
    assert!(
        !human.contains("no related test reaches the changed owner"),
        "{human}"
    );
    assert!(!human.contains("direct Jest/Vitest owner call"), "{human}");
    let finding = finding(&guard.root)?;
    let rows = finding
        .get("related_tests")
        .and_then(|value| value.as_array())
        .ok_or_else(|| format!("no related_tests: {finding}"))?;
    assert!(
        rows.iter()
            .any(|row| row.to_string().contains("import_path_affinity")),
        "the render must be an imported owner call: {finding}"
    );
    Ok(())
}

#[test]
fn a_render_with_no_assertion_never_credits_oracle_strength() -> Result<(), String> {
    let body = "import React from 'react';\nimport { render } from '@testing-library/react';\nimport { it } from 'vitest';\nimport { Badge } from './Badge';\nit('renders', () => { render(<Badge count={120} max={99} />); });\n";
    let guard = workspace(body)?;
    let finding = finding(&guard.root)?;
    let strength = finding.get("oracle_strength").and_then(|v| v.as_str());
    assert!(
        !matches!(strength, Some("strong")),
        "a render alone is not an oracle: {finding}"
    );
    let class = finding.get("classification").and_then(|v| v.as_str());
    assert_ne!(class, Some("exposed"), "{finding}");
    Ok(())
}

#[test]
fn a_same_named_component_from_another_module_is_not_the_owner() -> Result<(), String> {
    let body = RENDERED.replace("'./Badge'", "'../other/Badge'");
    let guard = workspace(&body)?;
    let finding = finding(&guard.root)?;
    let rows = finding
        .get("related_tests")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        !rows
            .iter()
            .any(|row| row.to_string().contains("import_path_affinity")),
        "another module's Badge must not reach the owner: {finding}"
    );
    Ok(())
}

#[test]
fn an_unrendered_owner_is_not_reached() -> Result<(), String> {
    let body = "import React from 'react';\nimport { render } from '@testing-library/react';\nimport { it, expect } from 'vitest';\nimport { Badge } from './Badge';\nit('mentions', () => { render(<div>Badge</div>); expect(typeof Badge).toBe('function'); });\n";
    let guard = workspace(body)?;
    let finding = finding(&guard.root)?;
    let rows = finding
        .get("related_tests")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        !rows
            .iter()
            .any(|row| row.to_string().contains("import_path_affinity")),
        "an unrendered owner is not an owner call: {finding}"
    );
    // A component with only a heuristic link is never told to call the function.
    let next = finding
        .get("recommended_next_step")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert!(!next.contains("add a direct owner call"), "{next}");
    assert!(
        next.contains("render the component"),
        "a heuristic-only component link advises a render: {next}"
    );
    Ok(())
}

#[test]
fn a_render_nested_directly_inside_another_element_is_reached() -> Result<(), String> {
    let body = RENDERED.replace(
        "render(<Badge count={120} max={99} />)",
        "render(<main><Badge count={120} max={99} /></main>)",
    );
    assert!(
        body.contains("<main><Badge "),
        "fixture must nest the render"
    );
    let guard = workspace(&body)?;
    let finding = finding(&guard.root)?;
    let rows = finding
        .get("related_tests")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        rows.iter()
            .any(|row| row.to_string().contains("import_path_affinity")),
        "a nested render is still an owner call: {finding}"
    );
    Ok(())
}

#[test]
fn a_render_with_an_unrelated_exact_assertion_is_not_exposed() -> Result<(), String> {
    let body = "import React from 'react';\nimport { render } from '@testing-library/react';\nimport { it, expect } from 'vitest';\nimport { Badge } from './Badge';\nit('renders', () => { render(<Badge count={120} max={99} />); expect(1 + 1).toBe(2); });\n";
    let guard = workspace(body)?;
    let finding = finding(&guard.root)?;
    let rows = finding
        .get("related_tests")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        rows.iter()
            .any(|row| row.to_string().contains("import_path_affinity")),
        "the render must reach the owner for this case to test strength: {finding}"
    );
    let class = finding.get("classification").and_then(|v| v.as_str());
    assert!(
        matches!(class, Some(class) if class != "exposed"),
        "an assertion unrelated to the render is not a discriminator: {finding}"
    );
    Ok(())
}
