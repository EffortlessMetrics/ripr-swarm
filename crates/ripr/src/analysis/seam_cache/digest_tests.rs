use super::*;
use serde::Serialize;
use sha2::{Digest, Sha256};

fn reference_digest<T: Serialize>(domain: &str, body: &T) -> Result<String, String> {
    let bytes = serde_json::to_vec(body).map_err(|error| error.to_string())?;
    let mut hash = Sha256::new();
    hash.update(domain.as_bytes());
    hash.update([0]);
    hash.update(bytes);
    Ok(format!("sha256:{:x}", hash.finalize()))
}

// Exact pre-buffering stream shape, retained as a differential performance
// control. Unlike the independent to_vec oracle it never retains a body Vec.
fn legacy_digest<T: Serialize>(domain: &str, body: &T) -> Result<String, String> {
    struct Writer(Sha256);
    impl std::io::Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Writer(Sha256::new());
    writer.0.update(domain.as_bytes());
    writer.0.update([0]);
    serde_json::to_writer(&mut writer, body).map_err(|error| error.to_string())?;
    Ok(format!("sha256:{:x}", writer.0.finalize()))
}

#[test]
fn digest_preserves_independent_byte_oracle_across_buffer_boundaries() -> Result<(), String> {
    // JSON quotes make the 8190-byte string exactly one 8192-byte buffer.
    for length in [0, 1, 63, 8190, 8191, 8192, 16382, 32769] {
        for domain in ["", "ripr-cache:FileFactCacheEnvelope:v1", "other\0domain"] {
            let body = "x".repeat(length);
            assert_eq!(
                semantic_body_digest(domain, &body)?,
                reference_digest(domain, &body)?
            );
        }
    }
    let body = serde_json::json!({
        "escaped": "\n\r\t\\\"雪🦀".repeat(4096),
        "nested": [null, true, false, -42, {"values": [0, 1, 2]}],
        "empty": []
    });
    assert_eq!(
        semantic_body_digest("nested", &body)?,
        reference_digest("nested", &body)?
    );
    assert_ne!(
        semantic_body_digest("a", &"bc")?,
        semantic_body_digest("ab", &"c")?
    );
    Ok(())
}

#[test]
fn digest_coalesces_token_writes_without_retaining_the_body() -> Result<(), String> {
    let body: Vec<u32> = (0..10000).collect();
    INTEGRITY_HASH_UPDATES.with(|count| count.set(0));
    assert_eq!(
        semantic_body_digest("tokens", &body)?,
        reference_digest("tokens", &body)?
    );
    let updates = INTEGRITY_HASH_UPDATES.with(std::cell::Cell::get);
    eprintln!("digest-token-updates={updates}");
    // ~49 KiB of small JSON tokens must reach SHA in bounded chunks.
    assert!(
        (1..=8).contains(&updates),
        "expected coalesced updates, got {updates}"
    );
    Ok(())
}

#[test]
fn digest_consumes_a_stream_before_the_whole_body_is_serialized() -> Result<(), String> {
    struct StreamedBody;
    impl Serialize for StreamedBody {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            use serde::ser::SerializeSeq as _;
            let mut sequence = serializer.serialize_seq(Some(64))?;
            let chunk = "x".repeat(1024);
            for index in 0..64 {
                sequence.serialize_element(&chunk)?;
                // A whole-body to_vec replacement has not fed SHA at this
                // point, although more than two fixed buffers were emitted.
                if index == 16 && INTEGRITY_HASH_UPDATES.with(std::cell::Cell::get) == 0 {
                    return Err(serde::ser::Error::custom(
                        "hash retained the serialized body",
                    ));
                }
            }
            sequence.end()
        }
    }
    INTEGRITY_HASH_UPDATES.with(|count| count.set(0));
    assert_eq!(
        semantic_body_digest("stream", &StreamedBody)?,
        reference_digest("stream", &vec!["x".repeat(1024); 64])?
    );
    Ok(())
}

#[test]
fn digest_refuses_serialization_failure_after_partial_buffer_flush() {
    struct FailsAfterBody;
    impl Serialize for FailsAfterBody {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            use serde::ser::SerializeSeq as _;
            let mut sequence = serializer.serialize_seq(Some(2))?;
            sequence.serialize_element(&"escaped\n雪".repeat(4096))?;
            Err(serde::ser::Error::custom(
                "intentional serialization refusal",
            ))
        }
    }
    assert_eq!(
        semantic_body_digest("failure", &FailsAfterBody),
        Err("encode integrity body failed: intentional serialization refusal".to_owned())
    );
}

#[test]
fn real_file_facts_keep_original_cache_bytes_and_integrity() -> Result<(), String> {
    use crate::analysis::syntax::RustSyntaxAdapter as _;
    let path = Path::new("src/digest_fixture.rs");
    let mut source = String::new();
    for index in 0..64 {
        source.push_str(&format!(
            "pub fn value_{index}(input: i32) -> i32 {{ if input > {index} {{ input + 1 }} else {{ input - 1 }} }}\n\
             #[test] fn checks_{index}() {{ assert_eq!(value_{index}({index} + 1), {index} + 2); }}\n"
        ));
    }
    let facts = crate::analysis::syntax::RaRustSyntaxAdapter.summarize_file(path, &source)?;
    assert_eq!(facts.functions.len(), 128);
    assert_eq!(facts.tests.len(), 64);
    let envelope =
        FileFactCacheEnvelope::new(RepoFileFactCacheKey::new(path, source.as_bytes()), facts);
    let digest = reference_digest("ripr-cache:FileFactCacheEnvelope:v1", &envelope)?;
    let expected = serde_json::to_vec_pretty(&ChecksummedBody {
        body: &envelope,
        payload_digest: digest,
    })
    .map_err(|error| error.to_string())?;
    let encoded = codec::encode_file_facts(&envelope)?;
    assert_eq!(encoded, expected);
    // Bounded alternating pairs profile the actual file-fact encoding route;
    // duration is reported, never used as a flaky pass/fail threshold.
    for sample in 0..7 {
        let legacy = || -> Result<Vec<u8>, String> {
            encode_integrity_body(
                &envelope,
                legacy_digest("ripr-cache:FileFactCacheEnvelope:v1", &envelope)?,
            )
        };
        let current = || codec::encode_file_facts(&envelope);
        let measure = |encode: &dyn Fn() -> Result<Vec<u8>, String>| -> Result<u128, String> {
            let started = std::time::Instant::now();
            let bytes = encode()?;
            let elapsed = started.elapsed().as_nanos();
            assert_eq!(bytes, expected);
            Ok(elapsed)
        };
        let (legacy_ns, current_ns) = if sample % 2 == 0 {
            (measure(&legacy)?, measure(&current)?)
        } else {
            let current_ns = measure(&current)?;
            (measure(&legacy)?, current_ns)
        };
        eprintln!(
            "file-fact-codec sample={sample} legacy_ns={legacy_ns} current_ns={current_ns} bytes={}",
            encoded.len()
        );
    }
    let mut loaded = codec::decode_file_facts(&encoded)?;
    loaded.validate_integrity()?;
    loaded.file_facts.source = "changed source".into();
    assert!(loaded.validate_integrity().is_err());
    Ok(())
}
