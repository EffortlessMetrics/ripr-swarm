//! Test registration is separate from execution and failure propagation.
//!
//! Conditions and runtime rebinding are not evaluated. Recognized controls
//! therefore withhold credit even when a conditional skip might be inactive.

use super::{PythonImport, PythonTest};

pub(super) fn activation_control(test: &PythonTest) -> Option<&str> {
    test.activation_controls.first().map(String::as_str)
}

pub(super) fn activation_controls(
    decorators: &[String],
    definition_imports: &[PythonImport],
) -> Vec<String> {
    decorators
        .iter()
        .filter(|decorator| is_activation_control(decorator, definition_imports))
        .cloned()
        .collect()
}

fn is_activation_control(decorator: &str, imports: &[PythonImport]) -> bool {
    if is_control_name(decorator) {
        return true;
    }
    let (root, tail) = decorator.split_once('.').unwrap_or((decorator, ""));
    // Avoid resolving ordinary fixture/parametrization metadata at every query.
    if !tail.is_empty()
        && !matches!(
            tail.rsplit('.').next(),
            Some("skip" | "skipIf" | "skipUnless" | "expectedFailure" | "skipif" | "xfail")
        )
    {
        return false;
    }
    imports
        .iter()
        .filter(|import| import.alias == root)
        .any(|import| {
            let mut resolved = if import.source_module.is_empty() {
                import.imported.clone()
            } else {
                format!("{}.{}", import.source_module, import.imported)
            };
            if !tail.is_empty() {
                resolved.push('.');
                resolved.push_str(tail);
            }
            is_control_name(&resolved)
        })
}

fn is_control_name(name: &str) -> bool {
    matches!(
        name,
        "unittest.skip"
            | "unittest.skipIf"
            | "unittest.skipUnless"
            | "unittest.expectedFailure"
            | "pytest.mark.skip"
            | "pytest.mark.skipif"
            | "pytest.mark.xfail"
    )
}
