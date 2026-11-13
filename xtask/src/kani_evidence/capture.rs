use super::*;

pub(super) struct Custody {
    pub(super) root: PathBuf,
    pub(super) directory: PathBuf,
    pub(super) relative: String,
    pub(super) bytes: u64,
    pub(super) invocations: Vec<Invocation>,
    pub(super) explicit_inputs: BTreeSet<String>,
    pub(super) package_inputs: BTreeMap<String, BTreeSet<String>>,
    pub(super) required_inputs: Vec<String>,
    pub(super) deadline: Option<Instant>,
    packages: Vec<String>,
}
impl Custody {
    pub(super) fn create(root: &Path, packages: &[String]) -> Result<Self, String> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let ci = std::env::var("GITHUB_RUN_ID")
            .ok()
            .zip(std::env::var("GITHUB_RUN_ATTEMPT").ok())
            .filter(|(id, attempt)| {
                !id.is_empty()
                    && !attempt.is_empty()
                    && id.bytes().all(|b| b.is_ascii_digit())
                    && attempt.bytes().all(|b| b.is_ascii_digit())
            });
        let run_id = ci
            .map(|(id, attempt)| format!("{id}-{attempt}/{}-{unique}", std::process::id()))
            .unwrap_or_else(|| format!("{}-{unique}", std::process::id()));
        let relative = format!("kani-evidence/{run_id}");
        let directory = root.join("target/ripr/reports").join(&relative);
        fs::create_dir_all(directory.parent().ok_or("artifact parent absent")?)
            .map_err(|e| e.to_string())?;
        // No reuse: a stale native list or earlier passing report cannot be this run.
        fs::create_dir(&directory).map_err(|e| format!("create fresh evidence directory: {e}"))?;
        Ok(Self {
            root: root.to_path_buf(),
            directory,
            relative,
            bytes: 0,
            invocations: Vec::new(),
            explicit_inputs: BTreeSet::new(),
            package_inputs: BTreeMap::new(),
            required_inputs: Vec::new(),
            deadline: None,
            packages: packages.to_vec(),
        })
    }
    pub(super) fn retain(&mut self, name: &str, bytes: &[u8]) -> Result<Artifact, String> {
        let next = self
            .bytes
            .checked_add(bytes.len() as u64)
            .ok_or("artifact byte counter overflow")?;
        if next > TOTAL_ARTIFACT_BYTES {
            return Err("native evidence exceeds the 96 MiB run budget".to_string());
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.directory.join(name))
            .map_err(|e| format!("create evidence {name}: {e}"))?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|e| format!("retain evidence {name}: {e}"))?;
        self.bytes = next;
        Ok(Artifact {
            path: name.to_string(),
            sha256: hash(bytes),
            bytes: bytes.len() as u64,
        })
    }
    pub(super) fn capture(
        &mut self,
        phase: &str,
        package: Option<&str>,
        program: &str,
        arguments: &[String],
        cwd: &Path,
        budget: (Duration, usize),
    ) -> Result<(String, String), String> {
        let (timeout, limit) = budget;
        let timeout = if phase == "after" {
            // Final source observation has its own bounded Git budget even
            // after a provider timeout; it cannot prolong verifier execution.
            timeout
        } else {
            self.deadline.map_or(timeout, |deadline| {
                timeout.min(deadline.saturating_duration_since(Instant::now()))
            })
        };
        let index = self.invocations.len();
        let mut invocation = Invocation {
            phase: phase.to_string(),
            package: package.map(str::to_string),
            program: program.to_string(),
            arguments: arguments.to_vec(),
            working_directory: cwd.display().to_string(),
            timeout_ms: timeout.as_millis().min(u128::from(u64::MAX)).max(1) as u64,
            stream_limit_bytes: limit,
            duration_ms: None,
            exit_code: None,
            native_status: None,
            timed_out: false,
            capture_complete: false,
            stdout: None,
            stderr: None,
            error: None,
        };
        if timeout.as_millis() == 0 {
            let error = "whole-run deadline exhausted before native invocation".to_string();
            invocation.error = Some(error.clone());
            self.invocations.push(invocation);
            return Err(error);
        }
        let output = run::capture_bytes_in_dir_with_env_budget_typed(
            Path::new(program),
            arguments,
            cwd,
            (&[("CARGO_TERM_COLOR", "never")], &[]),
            run::ByteCaptureBudget {
                timeout,
                stdout_bytes: limit,
                stderr_bytes: limit,
            },
            "Kani evidence capture",
        );
        let result = match output {
            Err(_) => {
                // The shared error can contain native text. Keep a fixed reason in
                // rendered output; absence of captured bytes is explicit.
                Err("native launch, bounded capture, or process cleanup failed; complete raw evidence unavailable".to_string())
            }
            Ok(output) => {
                invocation.duration_ms =
                    Some(output.duration.as_millis().min(u128::from(u64::MAX)) as u64);
                invocation.exit_code = output
                    .status
                    .as_ref()
                    .and_then(std::process::ExitStatus::code);
                invocation.native_status = output.status.as_ref().map(ToString::to_string);
                invocation.timed_out = output.timed_out;
                let retained = self
                    .retain(&format!("{index:02}-{phase}.stdout"), &output.stdout)
                    .and_then(|stdout| {
                        invocation.stdout = Some(stdout);
                        self.retain(&format!("{index:02}-{phase}.stderr"), &output.stderr)
                    });
                match retained {
                    Err(error) => Err(error),
                    Ok(stderr) => {
                        invocation.stderr = Some(stderr);
                        invocation.capture_complete = output.status.is_some();
                        String::from_utf8(output.stdout)
                            .and_then(|stdout| {
                                String::from_utf8(output.stderr).map(|stderr| (stdout, stderr))
                            })
                            .map_err(|error| {
                                format!(
                                    "native text is not strict UTF-8; exact bytes retained: {error}"
                                )
                            })
                    }
                }
            }
        };
        if let Err(error) = &result {
            invocation.error = Some(error.clone());
        }
        self.invocations.push(invocation);
        result
    }
    pub(super) fn git(&mut self, phase: &str, arguments: &[&str]) -> Result<String, String> {
        let root = self.root.clone();
        let args: Vec<_> = arguments.iter().map(|s| (*s).to_string()).collect();
        let (stdout, _) = self.capture(
            phase,
            None,
            "git",
            &args,
            &root,
            (
                Duration::from_secs(10),
                if arguments.first() == Some(&"ls-files") {
                    GIT_INVENTORY_STREAM
                } else {
                    SMALL_STREAM
                },
            ),
        )?;
        if !self.invocations.last().is_some_and(Invocation::succeeded) {
            return Err("source identity Git command did not complete successfully".to_string());
        }
        Ok(stdout)
    }
    pub(super) fn snapshot(&mut self, phase: &str) -> (Snapshot, BTreeMap<String, String>) {
        let mut snapshot = Snapshot {
            revision: None,
            tree: None,
            dirty: None,
            manifest: None,
            file_count: 0,
            error: None,
        };
        let mut files = BTreeMap::new();
        let mut explicit_inputs = BTreeSet::new();
        let mut package_inputs = BTreeMap::new();
        let result = (|| {
            snapshot.revision = Some(self.git(phase, &["rev-parse", "HEAD"])?.trim().to_string());
            snapshot.tree = Some(
                self.git(phase, &["rev-parse", "HEAD^{tree}"])?
                    .trim()
                    .to_string(),
            );
            snapshot.dirty = Some(
                !self
                    .git(
                        phase,
                        &["status", "--porcelain=v1", "--untracked-files=normal"],
                    )?
                    .is_empty(),
            );
            let listing = self.git(
                phase,
                &[
                    "ls-files",
                    "-z",
                    "--cached",
                    "--others",
                    "--exclude-standard",
                ],
            )?;
            let mut total = 0u64;
            let mut paths: BTreeSet<String> = listing
                .split('\0')
                .filter(|s| source_input(s))
                .map(str::to_string)
                .collect();
            paths.extend(self.required_inputs.iter().cloned());
            for (_, prefix) in PACKAGES {
                gather_source_files(&self.root, &self.root.join(prefix), &mut paths)?;
            }
            for prefix in ["xtask/src", "tools/repo-policy/src"] {
                gather_source_files(&self.root, &self.root.join(prefix), &mut paths)?;
            }
            for package in &self.packages {
                let inputs = formal::native::extend_source_inputs(
                    &self.root,
                    &mut paths,
                    std::slice::from_ref(package),
                )?;
                package_inputs.insert(package.clone(), inputs.clone());
                explicit_inputs.extend(inputs);
            }
            for name in &paths {
                if name.split('/').any(|c| c == "..") || name.starts_with('/') {
                    return Err("source path escaped repository".to_string());
                }
                let path = self.root.join(name);
                let mut component_path = self.root.clone();
                for component in Path::new(name).components() {
                    component_path.push(component);
                    if fs::symlink_metadata(&component_path)
                        .map_err(|e| e.to_string())?
                        .file_type()
                        .is_symlink()
                    {
                        return Err(format!("source path contains a symlink: {name}"));
                    }
                }
                let canonical = fs::canonicalize(&path).map_err(|e| e.to_string())?;
                if !canonical.starts_with(&self.root) {
                    return Err(format!("source path escapes repository: {name}"));
                }
                let bytes = read_bounded(&path, FILE_BYTES)?;
                total = total
                    .checked_add(bytes.len() as u64)
                    .ok_or("source byte counter overflow")?;
                if total > SOURCE_BYTES {
                    return Err("source closure exceeds 256 MiB capture budget".to_string());
                }
                files.insert(name.to_string(), hash(&bytes));
            }
            for required in ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml"] {
                if !files.contains_key(required) {
                    return Err(format!("source closure is missing {required}"));
                }
            }
            snapshot.file_count = files.len();
            snapshot.manifest = Some(self.retain(
                &format!("{phase}-source.json"),
                &formal::native::canonical_manifest_bytes(&files)?,
            )?);
            Ok(())
        })();
        if let Err(error) = result {
            snapshot.error = Some(error);
            // Partial in-memory hashes are not a retained source observation.
            // Neither declarations nor configuration may borrow them.
            snapshot.file_count = 0;
            files.clear();
        } else if phase == "before" {
            // These scopes belong to the retained baseline. A later snapshot
            // can expose drift, but cannot lend inputs to a failed baseline.
            self.explicit_inputs = explicit_inputs;
            self.package_inputs = package_inputs;
        }
        (snapshot, files)
    }
}

pub(super) fn source_input(name: &str) -> bool {
    matches!(name, "Cargo.toml" | "Cargo.lock" | "rust-toolchain.toml")
        || name.ends_with(".rs")
        || name.ends_with("Cargo.toml")
        || name == ".ripr/formal-claims.json"
        || name.starts_with("docs/formal-verification/")
        || name.starts_with(".cargo/")
        || name.starts_with("crates/")
        || name.starts_with("xtask/src/")
        || name == "xtask/Cargo.toml"
        || name.starts_with("tools/repo-policy/")
}

fn gather_source_files(
    root: &Path,
    path: &Path,
    files: &mut BTreeSet<String>,
) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "source symlink is outside the admitted closure: {}",
            path.display()
        ));
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
            gather_source_files(root, &entry.map_err(|e| e.to_string())?.path(), files)?;
        }
    } else if metadata.is_file() {
        files.insert(
            path.strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/"),
        );
        if files.len() > 20_000 {
            return Err("source closure exceeds 20,000 files".to_string());
        }
    } else {
        return Err("source closure contains a non-regular input".to_string());
    }
    Ok(())
}
