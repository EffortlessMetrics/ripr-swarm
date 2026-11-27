use super::{Reader, STDOUT_LIMIT, decode, sides};
use crate::reports::release_ledger::capture_tests::{commit, new_repo};
use crate::reports::release_ledger::inline_tests::side_at;
use std::fs;

#[test]
fn batch_frames_preserve_raw_bytes_and_reject_incomplete_responses() -> Result<(), String> {
    let oid = "a".repeat(40);
    let header = format!("{oid} blob 4\n");
    let frame = [header.as_bytes(), b"\xff\0\r\n\n"].concat();
    let second = format!("{oid} blob 0\n\n");
    let both = [frame.as_slice(), second.as_bytes()].concat();
    if decode(&both, 2) != Some(vec![b"\xff\0\r\n".as_slice(), b"".as_slice()]) {
        return Err("batch framing changed blob body, length, or order".into());
    }
    for (bytes, count) in [
        (frame.as_slice(), 2),
        (both.as_slice(), 1),
        (header.as_bytes(), 1),
        (b"revision:path missing\n", 1),
        (format!("{oid} tree 0\n\n").as_bytes(), 1),
        (format!("{oid} blob 4\nbytes").as_bytes(), 1),
    ] {
        if decode(bytes, count).is_some() {
            return Err("incomplete, surplus, non-blob or unframed response was accepted".into());
        }
    }
    Ok(())
}

#[test]
fn real_batch_and_fallback_preserve_sides_and_original_git_errors() -> Result<(), String> {
    let root = new_repo("inline-batch")?;
    let run = (|| {
        let path = "test path.rs";
        fs::write(
            root.join(path),
            b"// \xff\n#[cfg(test)]\nmod tests {\n\n}\n",
        )
        .map_err(|e| e.to_string())?;
        let first = commit(&root, &[], "original")?;
        fs::write(root.join("large"), vec![b'x'; STDOUT_LIMIT]).map_err(|e| e.to_string())?;
        let next = commit(&root, &[(path, Some("fn product() {}\n"))], "changed")?;
        let requests = [(next.as_str(), path), (first.as_str(), path)];
        let mut batch = Reader::new(&root, &requests);
        if batch.prepared.len() != 2 {
            return Err("an ordinary two-side request used individual git show calls".into());
        }
        for (rev, path) in requests {
            let skipped = batch.read(None, path)?;
            let got = batch.read(Some(rev), path)?;
            let old = side_at(&root, rev, path)?;
            if !skipped.spans.is_empty()
                || !skipped.blank.is_empty()
                || got.spans != old.spans
                || got.blank != old.blank
            {
                return Err(format!("different spans/blank lines for {rev}:{path}"));
            }
        }
        for path in ["large", "missing", "with\nnewline", "with\rcarriage"] {
            let requests = [(next.as_str(), path), (next.as_str(), "test path.rs")];
            if sides(&root, &requests).is_some() {
                return Err(format!("unsafe or oversized request was batched: {path:?}"));
            }
        }
        let requests = [(first.as_str(), path), (first.as_str(), "missing")];
        let mut fallback = Reader::new(&root, &requests);
        fallback.read(Some(&first), path)?;
        let actual = fallback.read(Some(&first), "missing").err();
        let old = side_at(&root, &first, "missing").err();
        if actual.is_none() || actual != old {
            return Err(format!(
                "missing-object fallback changed Git failure: {actual:?} vs {old:?}"
            ));
        }
        Ok(())
    })();
    fs::remove_dir_all(&root).map_err(|e| e.to_string())?;
    run
}
