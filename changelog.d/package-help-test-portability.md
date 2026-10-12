Keep the published rendered-help test target portable in an extracted Cargo
source package. Move live repository-guide audits into unpublished `xtask`,
retain shared flag validation, and check the real archive with a missing-helper
control. CLI behavior and the source-package include list are unchanged.
Retain the primary timeout diagnostic alongside every cleanup error, with
separate native timeout/descendant and cleanup-error controls.
