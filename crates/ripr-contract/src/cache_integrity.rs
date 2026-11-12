//! Language-neutral cache integrity codecs shared by adapter implementations.
//! Checksums detect corruption, not a writer capable of recomputing them.

#[cfg(test)]
std::thread_local! {
    static INTEGRITY_HASH_UPDATES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[derive(serde::Serialize)]
struct ChecksummedBody<'a, T> {
    #[serde(flatten)]
    body: &'a T,
    payload_digest: String,
}

/// Borrow the body as the established flattened checksummed wire envelope.
/// This view does not serialize, allocate a body copy, or authenticate its writer.
pub fn checksummed_body<T: serde::Serialize>(
    body: &T,
    payload_digest: String,
) -> impl serde::Serialize + '_ {
    ChecksummedBody {
        body,
        payload_digest,
    }
}

/// Stream the established pretty checksummed envelope into a caller-owned writer.
pub fn encode_checksummed_pretty_to_writer<T: serde::Serialize, W: std::io::Write>(
    body: &T,
    payload_digest: String,
    writer: W,
) -> Result<(), String> {
    serde_json::to_writer_pretty(
        writer,
        &ChecksummedBody {
            body,
            payload_digest,
        },
    )
    .map_err(|err| format!("encode checksummed cache body failed: {err}"))
}

/// Unkeyed checksum integrity does not authenticate a writer who can recompute the digest.
pub fn semantic_body_digest<T: serde::Serialize>(domain: &str, body: &T) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    // The serializer streams into SHA-256: warm admission never retains a second body.
    struct ChecksumWriter(Sha256);
    impl std::io::Write for ChecksumWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            #[cfg(test)]
            INTEGRITY_HASH_UPDATES.with(|count| count.set(count.get().saturating_add(1)));
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = ChecksumWriter(Sha256::new());
    writer.0.update(domain.as_bytes());
    writer.0.update([0]);
    // Coalesce tiny serializer fragments before SHA updates. Scratch stays
    // fixed-size; draining before finalization preserves every trailing byte.
    let mut writer = std::io::BufWriter::with_capacity(8 * 1024, writer);
    serde_json::to_writer(&mut writer, body)
        .map_err(|err| format!("encode integrity body failed: {err}"))?;
    let writer = writer
        .into_inner()
        .map_err(|err| format!("encode integrity body failed: {err}"))?;
    Ok(format!("sha256:{:x}", writer.0.finalize()))
}
/// Digest of an already-serialized body. Byte-identical to
/// [`semantic_body_digest`] of the value that produced `bytes` when `bytes`
/// is that value's compact serialization, so a reader can verify integrity
/// over the stored bytes it already read instead of re-serializing the
/// decoded body on every warm read.
pub fn semantic_digest_for_bytes(domain: &str, bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update([0]);
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}

/// Encode the established pretty checksummed envelope.
pub fn encode_integrity_body<T: serde::Serialize>(
    body: &T,
    payload_digest: String,
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    encode_checksummed_pretty_to_writer(body, payload_digest, &mut bytes)?;
    Ok(bytes)
}
/// [`encode_integrity_body`] without pretty-printing, for envelopes large
/// enough that indentation would dominate their size (the Python per-file
/// fact store, #10914, and Rust file facts, #9216). Decoding and the digest
/// are unchanged.
pub fn encode_integrity_body_compact<T: serde::Serialize>(
    body: &T,
    payload_digest: String,
) -> Result<Vec<u8>, String> {
    let bytes = serde_json::to_vec(&ChecksummedBody {
        body,
        payload_digest,
    })
    .map_err(|err| format!("encode checksummed cache body failed: {err}"))?;
    Ok(bytes)
}

#[cfg(test)]
mod digest_tests;
