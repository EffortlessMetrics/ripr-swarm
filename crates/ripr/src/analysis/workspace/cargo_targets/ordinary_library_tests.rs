use super::*;

struct LibraryFixture(PathBuf);

impl LibraryFixture {
    fn new() -> Result<Self, String> {
        let root = unique_workspace("ordinary-library");
        std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(root.join("tests")).map_err(|error| error.to_string())?;
        for file in [
            "src/lib.rs",
            "src/child.rs",
            "tests/plain.rs",
            "tests/custom.rs",
        ] {
            std::fs::write(root.join(file), "fn main() {}\n").map_err(|error| error.to_string())?;
        }
        Ok(Self(root))
    }

    fn manifest(&self, library: &str) -> Result<(), String> {
        std::fs::write(
            self.0.join("Cargo.toml"),
            format!(
                "[package]\nname='ordinary-library'\nversion='0.1.0'\nedition='2021'\n\
                 {library}\n\
                 [[test]]\nname='plain'\nharness=true\n\
                 [[test]]\nname='custom'\nharness=false\n"
            ),
        )
        .map_err(|error| error.to_string())
    }
}

impl Drop for LibraryFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn library_test_eligibility_preserves_integration_target_verdicts() -> Result<(), String> {
    let fixture = LibraryFixture::new()?;
    for (library, expected) in [
        ("", true),
        ("[lib]\nharness=false\n", false),
        ("[lib]\ntest=false\n", false),
        ("[lib]\nharness=true\ntest=true\n", true),
    ] {
        fixture.manifest(library)?;
        let mut inventory = ManifestInventory::default();
        assert_eq!(
            inventory.ordinary_library_test_target(&fixture.0, Path::new("src/lib.rs")),
            expected,
            "{library:?}"
        );
        assert!(!inventory.ordinary_library_test_target(&fixture.0, Path::new("src/child.rs")));
        assert!(!inventory.ordinary_library_test_target(&fixture.0, Path::new("tests/plain.rs")));
        assert_eq!(
            inventory.verdict(&fixture.0, Path::new("src/lib.rs")),
            CargoHarnessVerdict::NotDeclared,
            "library admission must not change integration-target identity"
        );
        assert_eq!(
            inventory.verdict(&fixture.0, Path::new("tests/plain.rs")),
            CargoHarnessVerdict::HarnessEnabled
        );
        assert_eq!(
            inventory.verdict(&fixture.0, Path::new("tests/custom.rs")),
            CargoHarnessVerdict::HarnessDisabled
        );
    }
    Ok(())
}

#[test]
fn library_target_metadata_refuses_missing_flags_kinds_and_feature_conditions() -> Result<(), String>
{
    let package = serde_json::json!({
        "id": "wire-id",
        "manifest_path": "/workspace/Cargo.toml",
        "features": {"default": [], "enabled": []},
        "targets": [{
            "name": "wire", "kind": ["lib"], "test": true,
            "src_path": "/workspace/src/lib.rs"
        }]
    });
    let target = &package["targets"][0];
    for (mutated, admitted) in [
        (target.clone(), true),
        (
            {
                let mut value = target.clone();
                value["test"] = serde_json::json!(false);
                value
            },
            false,
        ),
        (
            {
                let mut value = target.clone();
                value.as_object_mut().ok_or("target object")?.remove("test");
                value
            },
            false,
        ),
        (
            {
                let mut value = target.clone();
                value["kind"] = serde_json::json!(["test"]);
                value
            },
            false,
        ),
        (
            {
                let mut value = target.clone();
                value["kind"] = serde_json::json!(["lib", "test"]);
                value
            },
            false,
        ),
        (
            {
                let mut value = target.clone();
                value["required-features"] = serde_json::json!(["enabled"]);
                value
            },
            false,
        ),
    ] {
        let mut package = package.clone();
        package["targets"] = serde_json::json!([mutated]);
        let metadata = serde_json::json!({
            "packages": [package], "workspace_default_members": ["wire-id"]
        });
        let libraries = workspace_target_owners(&metadata, MetadataTargetKind::OrdinaryLibrary);
        assert_eq!(
            libraries.values().flatten().any(|owner| owner.test_enabled),
            admitted,
            "{metadata}"
        );
    }
    Ok(())
}

#[test]
fn library_target_ambiguity_or_missing_metadata_grants_nothing() -> Result<(), String> {
    let fixture = LibraryFixture::new()?;
    fixture.manifest("")?;
    let target = serde_json::json!({
        "name": "wire", "kind": ["lib"], "test": true,
        "src_path": fixture.0.join("src/lib.rs")
    });
    let metadata = serde_json::json!({
        "workspace_default_members": ["a", "b"],
        "packages": [
            {"id": "a", "manifest_path": fixture.0.join("Cargo.toml"), "targets": [target.clone()]},
            {"id": "b", "manifest_path": fixture.0.join("other/Cargo.toml"), "targets": [target]}
        ]
    });
    let libraries = workspace_target_owners(&metadata, MetadataTargetKind::OrdinaryLibrary);
    assert_eq!(
        libraries.values().flatten().count(),
        2,
        "ambiguous owners must reach the gate"
    );
    let mut inventory = ManifestInventory {
        metadata: MetadataState::Loaded(MetadataTargetOwners {
            libraries,
            tests: BTreeMap::new(),
        }),
        ..ManifestInventory::default()
    };
    assert!(!inventory.ordinary_library_test_target(&fixture.0, Path::new("src/lib.rs")));
    inventory.metadata = MetadataState::Failed;
    assert!(!inventory.ordinary_library_test_target(&fixture.0, Path::new("src/lib.rs")));
    Ok(())
}

#[test]
fn library_default_member_selection_does_not_borrow_an_unselected_target() -> Result<(), String> {
    let fixture = LibraryFixture::new()?;
    std::fs::write(
        fixture.0.join("Cargo.toml"),
        "[workspace]\nmembers=['selected','unselected']\ndefault-members=['selected']\nresolver='2'\n",
    )
    .map_err(|error| error.to_string())?;
    for member in ["selected", "unselected"] {
        let root = fixture.0.join(member);
        std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            root.join("Cargo.toml"),
            format!("[package]\nname='{member}'\nversion='0.1.0'\nedition='2021'\n"),
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(root.join("src/lib.rs"), "#[test] fn collected() {}\n")
            .map_err(|error| error.to_string())?;
    }
    let mut inventory = ManifestInventory::default();
    assert!(inventory.ordinary_library_test_target(&fixture.0, Path::new("selected/src/lib.rs")));
    assert!(
        !inventory.ordinary_library_test_target(&fixture.0, Path::new("unselected/src/lib.rs"))
    );
    Ok(())
}
