use super::*;

pub(super) fn declared(
    root: &Path,
    sources: &BTreeMap<String, String>,
) -> Result<Vec<Subject>, String> {
    let mut out = Vec::new();
    for (package, prefix) in PACKAGES {
        let files = sources
            .keys()
            .filter(|p| p.starts_with(prefix) && p.ends_with(".rs"))
            .map(|name| {
                read_bounded(&root.join(name), FILE_BYTES)
                    .and_then(|b| String::from_utf8(b).map_err(|e| e.to_string()))
                    .map(|text| (name.clone(), text))
            })
            .collect::<Result<Vec<_>, _>>()?;
        for harness in lane::discover_harnesses(&files) {
            out.push(Subject {
                package: package.to_string(),
                source_digest: sources[&harness.file].clone(),
                file: harness.file,
                name: harness.name,
                qualified_name: None,
            });
        }
    }
    out.sort();
    Ok(out)
}
