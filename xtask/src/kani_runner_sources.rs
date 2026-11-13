//! Exact source commitments compiled into both maintained Kani frontdoors.
//! This detects a stale collector checkout; it is not a binary signature or a
//! claim that Rust/Cargo dependency code, build scripts or the compiler are proved.
pub const SOURCES: &[(&str, &[u8])] = &[
    ("Cargo.toml", include_bytes!("../../Cargo.toml")),
    ("Cargo.lock", include_bytes!("../../Cargo.lock")),
    (
        "rust-toolchain.toml",
        include_bytes!("../../rust-toolchain.toml"),
    ),
    ("xtask/Cargo.toml", include_bytes!("../Cargo.toml")),
    (
        "xtask/src/kani_runner_sources.rs",
        include_bytes!("kani_runner_sources.rs"),
    ),
    (
        "xtask/src/bin/formal-kani.rs",
        include_bytes!("bin/formal-kani.rs"),
    ),
    ("xtask/src/lib.rs", include_bytes!("lib.rs")),
    ("xtask/src/main.rs", include_bytes!("main.rs")),
    (
        "xtask/src/formal_process.rs",
        include_bytes!("formal_process.rs"),
    ),
    (
        "xtask/src/kani_evidence.rs",
        include_bytes!("kani_evidence.rs"),
    ),
    (
        "xtask/src/kani_evidence/capture.rs",
        include_bytes!("kani_evidence/capture.rs"),
    ),
    (
        "xtask/src/kani_evidence/configuration.rs",
        include_bytes!("kani_evidence/configuration.rs"),
    ),
    (
        "xtask/src/kani_evidence/inventory.rs",
        include_bytes!("kani_evidence/inventory.rs"),
    ),
    (
        "xtask/src/kani_evidence/registration.rs",
        include_bytes!("kani_evidence/registration.rs"),
    ),
    (
        "xtask/src/kani_evidence/publication.rs",
        include_bytes!("kani_evidence/publication.rs"),
    ),
    ("xtask/src/kani_lane.rs", include_bytes!("kani_lane.rs")),
    ("xtask/src/ansi.rs", include_bytes!("ansi.rs")),
    ("xtask/src/run.rs", include_bytes!("run.rs")),
    (
        "xtask/src/run/proc_stat.rs",
        include_bytes!("run/proc_stat.rs"),
    ),
    (
        "xtask/src/run/command_diagnostic.rs",
        include_bytes!("run/command_diagnostic.rs"),
    ),
    (
        "xtask/src/formal_evidence.rs",
        include_bytes!("formal_evidence.rs"),
    ),
    (
        "xtask/src/formal_evidence/native.rs",
        include_bytes!("formal_evidence/native.rs"),
    ),
    (
        "xtask/src/formal_evidence/native/replay.rs",
        include_bytes!("formal_evidence/native/replay.rs"),
    ),
    (
        "xtask/src/formal_evidence/native/profile.rs",
        include_bytes!("formal_evidence/native/profile.rs"),
    ),
    (
        "xtask/src/formal_evidence/native/registered.rs",
        include_bytes!("formal_evidence/native/registered.rs"),
    ),
    (
        "xtask/src/formal_evidence/native/source_inputs.rs",
        include_bytes!("formal_evidence/native/source_inputs.rs"),
    ),
    (
        "xtask/src/formal_evidence/kani_output.rs",
        include_bytes!("formal_evidence/kani_output.rs"),
    ),
    (
        "xtask/src/formal_evidence/native_inventory.rs",
        include_bytes!("formal_evidence/native_inventory.rs"),
    ),
    (
        "xtask/src/formal_registry.rs",
        include_bytes!("formal_registry.rs"),
    ),
    (
        "xtask/src/formal_registry/prepare.rs",
        include_bytes!("formal_registry/prepare.rs"),
    ),
    (
        "xtask/src/formal_registry/control_pair.rs",
        include_bytes!("formal_registry/control_pair.rs"),
    ),
    (
        "xtask/src/formal_registry/scoreboard.rs",
        include_bytes!("formal_registry/scoreboard.rs"),
    ),
    (
        "xtask/src/formal_registry/transition.rs",
        include_bytes!("formal_registry/transition.rs"),
    ),
    (
        "crates/ripr/src/process_owner.rs",
        include_bytes!("../../crates/ripr/src/process_owner.rs"),
    ),
    (
        "crates/ripr-contract/src/analysis/cancellation.rs",
        include_bytes!("../../crates/ripr-contract/src/analysis/cancellation.rs"),
    ),
];
