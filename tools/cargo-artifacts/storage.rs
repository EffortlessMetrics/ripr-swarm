use super::*;

pub(super) fn build_plan(target_directory: &Path, max_size_bytes: u64) -> Result<CleanupPlan> {
    let mut selected = Vec::new();
    let mut selected_measurement = Measurement::default();
    let mut retained_measurement = Measurement::default();

    match fs::symlink_metadata(target_directory) {
        Ok(metadata) => require_plain_directory(target_directory, &metadata)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(CleanupPlan {
                target_directory: target_directory.to_path_buf(),
                selected,
                selected_measurement,
                retained_measurement,
                max_size_bytes,
                over_budget_bytes: 0,
            });
        }
        Err(error) => {
            return Err(ArtifactError::new(format!(
                "inspect target directory {}: {error}",
                target_directory.display()
            )))
        }
    }

    let entries = fs::read_dir(target_directory).map_err(|error| {
        ArtifactError::new(format!(
            "read target directory {}: {error}",
            target_directory.display()
        ))
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            ArtifactError::new(format!(
                "read an entry under {}: {error}",
                target_directory.display()
            ))
        })?;
        let name = entry.file_name();
        let path = entry.path();
        if is_preserved_entry(&name) {
            retained_measurement.add(measure_path(&path)?);
            continue;
        }
        let name = name.into_string().map_err(|name| {
            ArtifactError::new(format!(
                "refusing non-UTF-8 target entry under {}: {:?}",
                target_directory.display(),
                name
            ))
        })?;
        require_direct_child(target_directory, &path)?;
        let measurement = measure_path(&path)?;
        selected_measurement.add(measurement);
        selected.push(PlannedEntry {
            name,
            path,
            measurement,
        });
    }
    selected.sort_by(|left, right| left.name.cmp(&right.name));
    let over_budget_bytes = selected_measurement.bytes.saturating_sub(max_size_bytes);
    Ok(CleanupPlan {
        target_directory: target_directory.to_path_buf(),
        selected,
        selected_measurement,
        retained_measurement,
        max_size_bytes,
        over_budget_bytes,
    })
}

pub(super) fn is_preserved_entry(name: &OsStr) -> bool {
    name.to_str()
        .is_some_and(|value| value.eq_ignore_ascii_case(PRESERVED_ENTRY))
}

pub(super) fn measure_path(path: &Path) -> Result<Measurement> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        ArtifactError::new(format!("inspect {}: {error}", path.display()))
    })?;
    require_plain_path(path, &metadata)?;
    if metadata.is_file() {
        return Ok(Measurement {
            bytes: metadata.len(),
            files: 1,
            directories: 0,
        });
    }
    if !metadata.is_dir() {
        return Err(ArtifactError::new(format!(
            "refusing special filesystem entry {}",
            path.display()
        )));
    }

    let mut measurement = Measurement {
        bytes: 0,
        files: 0,
        directories: 1,
    };
    let entries = fs::read_dir(path)
        .map_err(|error| ArtifactError::new(format!("read {}: {error}", path.display())))?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            ArtifactError::new(format!("read an entry under {}: {error}", path.display()))
        })?;
        measurement.add(measure_path(&entry.path())?);
    }
    Ok(measurement)
}

pub(super) fn require_plain_path(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    if metadata.file_type().is_symlink() || is_reparse_point(metadata) {
        Err(ArtifactError::new(format!(
            "refusing symlink or reparse point {}",
            path.display()
        )))
    } else {
        Ok(())
    }
}

pub(super) fn require_plain_directory(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    require_plain_path(path, metadata)?;
    if metadata.is_dir() {
        Ok(())
    } else {
        Err(ArtifactError::new(format!(
            "expected a directory at {}",
            path.display()
        )))
    }
}

#[cfg(windows)]
pub(super) fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
pub(super) fn is_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

pub(super) fn require_direct_child(target_directory: &Path, path: &Path) -> Result<()> {
    if path.parent() != Some(target_directory)
        || path.file_name().is_none()
        || path.file_name().is_some_and(is_preserved_entry)
    {
        return Err(ArtifactError::new(format!(
            "refusing deletion outside direct non-ripr target ownership: {}",
            path.display()
        )));
    }
    Ok(())
}

pub(super) fn quarantine_planned_entry(
    target_directory: &Path,
    entry: &PlannedEntry,
    sequence: u64,
) -> Result<PathBuf> {
    require_direct_child(target_directory, &entry.path)?;
    let metadata = fs::symlink_metadata(&entry.path).map_err(|error| {
        ArtifactError::new(format!("recheck {}: {error}", entry.path.display()))
    })?;
    require_plain_path(&entry.path, &metadata)?;
    let _ = measure_path(&entry.path)?;

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let quarantine = target_directory.join(format!(
        ".cargo-artifacts-delete-{}-{nonce}-{sequence}",
        process::id()
    ));
    require_direct_child(target_directory, &quarantine)?;
    match fs::symlink_metadata(&quarantine) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Ok(_) => {
            return Err(ArtifactError::new(format!(
                "quarantine path already exists: {}",
                quarantine.display()
            )))
        }
        Err(error) => {
            return Err(ArtifactError::new(format!(
                "inspect quarantine path {}: {error}",
                quarantine.display()
            )))
        }
    }

    fs::rename(&entry.path, &quarantine).map_err(|error| {
        ArtifactError::new(format!(
            "quarantine {} as {}: {error}",
            entry.path.display(),
            quarantine.display()
        ))
    })?;
    let quarantined_metadata = fs::symlink_metadata(&quarantine).map_err(|error| {
        ArtifactError::new(format!(
            "inspect quarantined {} from {}: {error}",
            quarantine.display(),
            entry.path.display()
        ))
    })?;
    require_plain_path(&quarantine, &quarantined_metadata)?;
    let _ = measure_path(&quarantine)?;
    Ok(quarantine)
}

pub(super) fn remove_planned_entry(
    target_directory: &Path,
    entry: &PlannedEntry,
    sequence: u64,
) -> Result<()> {
    let quarantine = quarantine_planned_entry(target_directory, entry, sequence)?;
    let metadata = fs::symlink_metadata(&quarantine).map_err(|error| {
        ArtifactError::new(format!("recheck quarantine {}: {error}", quarantine.display()))
    })?;
    require_plain_path(&quarantine, &metadata)?;
    if metadata.is_dir() {
        fs::remove_dir_all(&quarantine).map_err(|error| {
            ArtifactError::new(format!(
                "remove quarantined {} from {}: {error}",
                quarantine.display(),
                entry.path.display()
            ))
        })
    } else if metadata.is_file() {
        fs::remove_file(&quarantine).map_err(|error| {
            ArtifactError::new(format!(
                "remove quarantined {} from {}: {error}",
                quarantine.display(),
                entry.path.display()
            ))
        })
    } else {
        Err(ArtifactError::new(format!(
            "refusing special quarantined filesystem entry {} from {}",
            quarantine.display(),
            entry.path.display()
        )))
    }
}

pub(super) fn write_receipts(
    target_directory: &Path,
    options: &Options,
    plan: &CleanupPlan,
    state: &ApplyState,
) -> Result<()> {
    let reports = ensure_report_directory(target_directory)?;
    write_plain_file(&reports.join(REPORT_JSON), &render_json(options, plan, state))?;
    write_plain_file(
        &reports.join(REPORT_MARKDOWN),
        &render_markdown(options, plan, state),
    )
}

pub(super) fn ensure_report_directory(target_directory: &Path) -> Result<PathBuf> {
    ensure_plain_directory(target_directory)?;
    let ripr = target_directory.join(PRESERVED_ENTRY);
    ensure_plain_directory(&ripr)?;
    let reports = ripr.join("reports");
    ensure_plain_directory(&reports)?;
    Ok(reports)
}

pub(super) fn ensure_plain_directory(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => require_plain_directory(path, &metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|create_error| {
                ArtifactError::new(format!("create directory {}: {create_error}", path.display()))
            })?;
            let metadata = fs::symlink_metadata(path).map_err(|inspect_error| {
                ArtifactError::new(format!(
                    "inspect created directory {}: {inspect_error}",
                    path.display()
                ))
            })?;
            require_plain_directory(path, &metadata)
        }
        Err(error) => Err(ArtifactError::new(format!(
            "inspect directory {}: {error}",
            path.display()
        ))),
    }
}

pub(super) fn write_plain_file(path: &Path, content: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            require_plain_path(path, &metadata)?;
            if !metadata.is_file() {
                return Err(ArtifactError::new(format!(
                    "refusing non-regular receipt path {}",
                    path.display()
                )));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(ArtifactError::new(format!(
                "inspect receipt path {}: {error}",
                path.display()
            )))
        }
    }
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .map_err(|error| ArtifactError::new(format!("open receipt {}: {error}", path.display())))?;
    file.write_all(content.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|error| ArtifactError::new(format!("write receipt {}: {error}", path.display())))
}
