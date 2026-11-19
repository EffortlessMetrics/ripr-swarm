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
fn real_file_facts_keep_compact_cache_bytes_and_integrity() -> Result<(), String> {
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
    let key = RepoFileFactCacheKey::new(path, source.as_bytes());
    // The borrowing write path must emit the owned envelope's exact digest
    // and bytes without cloning the facts.
    let borrowed = FileFactCacheEnvelopeRef::new(&key, &facts);
    let borrowed_bytes = codec::encode_file_facts_ref(&borrowed)?;
    let borrowed_digest = borrowed.expected_digest()?;
    let envelope = FileFactCacheEnvelope::new(key, facts);
    assert_eq!(borrowed_digest, envelope.expected_digest()?);
    assert_eq!(borrowed_bytes, codec::encode_file_facts(&envelope)?);
    let digest = reference_digest("ripr-cache:FileFactCacheEnvelope:v1", &envelope)?;
    // The stored entry is `<digest>\n<compact body>`, and the framed digest
    // over those body bytes is the same semantic digest the re-serializing
    // definition gives for the decoded envelope (#9216).
    let body = serde_json::to_vec(&envelope).map_err(|error| error.to_string())?;
    let mut expected = digest.clone().into_bytes();
    expected.push(b'\n');
    expected.extend_from_slice(&body);
    let encoded = codec::encode_file_facts(&envelope)?;
    assert_eq!(encoded, expected);
    assert_eq!(
        semantic_digest_for_bytes("ripr-cache:FileFactCacheEnvelope:v1", &body),
        digest
    );
    let loaded = codec::decode_file_facts(&encoded)?;
    assert!(loaded.digest_matches);
    assert_eq!(loaded.envelope.expected_digest()?, digest);
    // A body edited behind the stored digest decodes for its identity but
    // fails the digest, so it is never served.
    let tampered = String::from_utf8(encoded.clone())
        .map_err(|error| error.to_string())?
        .replacen("value_0", "value_X", 1);
    let tampered = codec::decode_file_facts(tampered.as_bytes())?;
    assert!(!tampered.digest_matches);
    // An entry without the digest header falls back to the embedded digest,
    // which this body lacks, so it is never served.
    let unframed = codec::decode_file_facts(&body)?;
    assert!(!unframed.digest_matches);
    Ok(())
}
