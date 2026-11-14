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
fn encoding_views_preserve_wire_bytes_and_digest() -> Result<(), String> {
    #[derive(Serialize)]
    struct Body {
        name: &'static str,
        values: [u32; 2],
    }
    let body = Body {
        name: "a\nb",
        values: [1, 2],
    };
    let digest = semantic_body_digest("wire", &body)?;
    let raw = br#"{"name":"a\nb","values":[1,2]}"#;
    assert_eq!(semantic_digest_for_bytes("wire", raw), digest);
    assert_ne!(semantic_digest_for_bytes("other", raw), digest);
    let compact = format!(r#"{{"name":"a\nb","values":[1,2],"payload_digest":"{digest}"}}"#);
    let pretty = format!(
        "{{\n  \"name\": \"a\\nb\",\n  \"values\": [\n    1,\n    2\n  ],\n  \"payload_digest\": \"{digest}\"\n}}"
    );
    assert_eq!(
        encode_integrity_body_compact(&body, digest.clone())?,
        compact.as_bytes()
    );
    assert_eq!(
        encode_integrity_body(&body, digest.clone())?,
        pretty.as_bytes()
    );
    let mut streamed = Vec::new();
    encode_checksummed_pretty_to_writer(&body, digest.clone(), &mut streamed)?;
    assert_eq!(streamed, pretty.as_bytes());
    let view =
        serde_json::to_vec(&checksummed_body(&body, digest)).map_err(|error| error.to_string())?;
    assert_eq!(view, compact.as_bytes());
    Ok(())
}

#[test]
fn encoder_refuses_writer_and_serialization_failures() {
    struct Refuses;
    impl std::io::Write for Refuses {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("writer refused"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let body = serde_json::json!({"key": 1});
    assert_eq!(
        encode_checksummed_pretty_to_writer(&body, "digest".to_owned(), Refuses),
        Err("encode checksummed cache body failed: writer refused".to_owned())
    );
    struct RefusesSerialization;
    impl Serialize for RefusesSerialization {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("body refused"))
        }
    }
    for result in [
        encode_integrity_body(&RefusesSerialization, "digest".to_owned()),
        encode_integrity_body_compact(&RefusesSerialization, "digest".to_owned()),
    ] {
        assert_eq!(
            result,
            Err("encode checksummed cache body failed: body refused".to_owned())
        );
    }
}
