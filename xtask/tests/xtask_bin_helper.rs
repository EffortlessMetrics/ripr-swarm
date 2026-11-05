// Keep the helper self-tests in one integration binary.
include!("common/xtask_bin.rs");

mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn bounded_not_found_retry_constants_match_windows_advisory_launch() {
        assert_eq!(MAX_NOT_FOUND_ATTEMPTS, 20);
        assert_eq!(NOT_FOUND_BACKOFF, Duration::from_millis(500));
    }

    #[test]
    fn canonical_xtask_path_launches() -> Result<(), String> {
        let output = output(|command| {
            command.arg("help");
        })?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "xtask help failed\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }

    #[test]
    fn retries_a_single_launch_not_found_before_success() -> Result<(), String> {
        static CALLS: AtomicU32 = AtomicU32::new(0);
        CALLS.store(0, Ordering::Relaxed);
        let output = output_with_launch(
            |command| {
                command.arg("help");
            },
            |command| {
                let call = CALLS.fetch_add(1, Ordering::Relaxed) + 1;
                if call == 1 {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        "temporarily missing xtask.exe",
                    ));
                }
                command.output()
            },
        )?;
        if CALLS.load(Ordering::Relaxed) != 2 {
            return Err(format!(
                "expected exactly two launch attempts, got {}",
                CALLS.load(Ordering::Relaxed)
            ));
        }
        if output.status.success() {
            Ok(())
        } else {
            Err("second launch should succeed".into())
        }
    }

    #[test]
    fn stops_after_the_not_found_retry_budget() -> Result<(), String> {
        static CALLS: AtomicU32 = AtomicU32::new(0);
        CALLS.store(0, Ordering::Relaxed);
        let error = match output_with_launch(
            |_| {},
            |_| {
                CALLS.fetch_add(1, Ordering::Relaxed);
                Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "still missing xtask.exe",
                ))
            },
        ) {
            Ok(_) => return Err("persistent NotFound must fail".into()),
            Err(error) => error,
        };
        assert_eq!(
            CALLS.load(Ordering::Relaxed),
            MAX_NOT_FOUND_ATTEMPTS + 1,
            "retry budget should allow one initial try plus {MAX_NOT_FOUND_ATTEMPTS} retries"
        );
        assert!(
            error.contains("failed to run xtask"),
            "unexpected error: {error}"
        );
        Ok(())
    }
}
