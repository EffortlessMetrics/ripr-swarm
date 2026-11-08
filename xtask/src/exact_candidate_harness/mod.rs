//! Shared exact-candidate execution and evidence boundary (#4510).

mod admission;
mod controls;
mod launch;
pub(crate) mod model;
pub(crate) mod packet;
mod process;

pub(crate) use admission::{
    AdmitOptions, admit_installed_binary, hash_file, snapshot_admitted_binary,
};
pub(crate) use controls::execution_boundary_controls;
pub(crate) use launch::{
    CHILD_OUTPUT_BYTES, CHILD_PATH, ROOT_TOKEN, child_env_for_root, cleanup, cleanup_with,
    environment_boundary, removed_env,
};

#[cfg(test)]
mod tests;
