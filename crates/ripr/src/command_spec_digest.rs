use crate::domain::{CommandSpec, CommandSpecDigest};
use sha2::{Digest, Sha256};

impl CommandSpecDigest for CommandSpec {
    /// Hash the typed route without its human display (#3999). The display
    /// carries the concrete checkout root a producer bound for copy/paste
    /// (#3948), while argv, cwd and expected writes stay root-relative, so
    /// hashing the display would give equivalent checkouts different
    /// identities. Consumers never reconstruct argv from the display, so it
    /// is not part of what the digest certifies.
    fn command_spec_sha256(&self) -> Result<String, String> {
        let mut identity = self.clone();
        identity.display = String::new();
        let bytes = serde_json::to_vec(&identity).map_err(|error| error.to_string())?;
        let digest = Sha256::digest(bytes);
        Ok(format!("sha256:{digest:x}"))
    }
}
