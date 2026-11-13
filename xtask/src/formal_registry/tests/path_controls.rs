use super::*;
use std::os::unix::fs::symlink;

#[test]
fn registry_rejects_symlinked_directory_components() -> RegistryResult<()> {
    for relative in [
        "src",
        "docs",
        "docs/specs",
        "policy/formal-claims.d",
        "policy",
    ] {
        let fixture = Fixture::new()?;
        let outside = Fixture::new()?;
        let claim = fixture.claim("example.actionability", "Only ready is actionable.")?;
        let path = fixture.write_registry(&claim, 1)?;
        load::load_registry(&path)?;

        let original = fixture.0.join(relative);
        let external = outside.0.join("relocated");
        fs::rename(&original, &external).map_err(|error| error.to_string())?;
        symlink(&external, &original).map_err(|error| error.to_string())?;
        let error = refusal(load::load_registry(&path))?;
        assert!(
            error.contains("non-symlink directory"),
            "{relative}: {error}"
        );

        fs::remove_file(&original).map_err(|error| error.to_string())?;
        fs::rename(&external, &original).map_err(|error| error.to_string())?;
        load::load_registry(&path)?;
    }
    Ok(())
}
