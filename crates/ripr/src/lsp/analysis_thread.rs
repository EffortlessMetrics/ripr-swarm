//! One long-lived thread for every LSP analysis refresh.
//!
//! Refreshes used to run through `tokio::task::spawn_blocking`, which hands
//! each one to whichever blocking-pool thread is free. glibc gives each
//! thread its own malloc arena and keeps freed pages in it, so after a few
//! refreshes landed on different threads the server held one analysis peak
//! per arena: on ripr-swarm's 15-commit diff RSS grew from ~590 MB after the
//! first refresh to ~1.15 GB after the second, while a single-arena run stayed
//! at ~635 MB. Running every refresh on one thread keeps it in one arena.
//!
//! Refreshes are already serialized by the scheduler's execution gate, so a
//! single thread costs no concurrency. A job that panics takes the thread
//! down with it; its caller sees a task failure, as with `spawn_blocking`,
//! and the next job starts a fresh thread.

use std::sync::Mutex;
use std::sync::mpsc;

type Job = Box<dyn FnOnce() + Send + 'static>;

pub(super) struct AnalysisThread {
    sender: Mutex<Option<mpsc::Sender<Job>>>,
}

impl Default for AnalysisThread {
    fn default() -> Self {
        Self {
            sender: Mutex::new(None),
        }
    }
}

impl AnalysisThread {
    /// Runs `job` on the analysis thread and waits for its result without
    /// blocking the async runtime.
    pub(super) async fn run<T, F>(&self, job: F) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        let (result_sender, result) = tokio::sync::oneshot::channel();
        self.submit(Box::new(move || {
            // The receiver is gone only when the refresh was dropped; there
            // is nobody left to tell.
            let _ = result_sender.send(job());
        }))?;
        result.await.map_err(|stopped| {
            format!(
                "the analysis thread stopped before returning a result ({stopped}; the analysis \
                 panicked); the next refresh starts a new analysis thread"
            )
        })
    }

    fn submit(&self, job: Job) -> Result<(), String> {
        let mut sender = match self.sender.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let job = match sender.as_ref() {
            Some(live) => match live.send(job) {
                Ok(()) => return Ok(()),
                // The previous thread ended after a panic: start another.
                Err(mpsc::SendError(job)) => job,
            },
            None => job,
        };
        let (fresh, jobs) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("ripr-lsp-analysis".to_owned())
            .spawn(move || {
                while let Ok(job) = jobs.recv() {
                    job();
                }
            })
            .map_err(|err| {
                format!(
                    "could not start the ripr analysis thread ({err}); \
                     free system threads or memory, then save again to retry"
                )
            })?;
        fresh.send(job).map_err(|unsent| {
            format!("the new ripr analysis thread exited before taking work ({unsent})")
        })?;
        *sender = Some(fresh);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::AnalysisThread;

    fn runtime() -> Result<tokio::runtime::Runtime, String> {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .map_err(|err| err.to_string())
    }

    #[test]
    fn every_job_runs_on_the_same_thread() -> Result<(), String> {
        let worker = AnalysisThread::default();
        runtime()?.block_on(async {
            let first = worker.run(|| std::thread::current().id()).await?;
            let second = worker.run(|| std::thread::current().id()).await?;
            assert_eq!(first, second);
            assert_ne!(first, std::thread::current().id());
            Ok(())
        })
    }

    #[test]
    fn a_panicking_job_fails_its_caller_and_the_next_job_still_runs() -> Result<(), String> {
        let worker = AnalysisThread::default();
        runtime()?.block_on(async {
            let before = worker.run(|| std::thread::current().id()).await?;
            // A runtime index failure stands in for an analysis bug; the index
            // is derived at runtime so the compiler cannot const-prove it.
            let out_of_bounds = std::env::args_os().count() + 1;
            let Err(err) = worker
                .run(move || {
                    let slots = [0u8];
                    slots[out_of_bounds]
                })
                .await
            else {
                return Err("a panicking job must report a failure".to_owned());
            };
            assert!(err.contains("analysis panicked"), "{err}");
            let after = worker.run(|| std::thread::current().id()).await?;
            assert_ne!(
                before, after,
                "a fresh thread replaces the one that panicked"
            );
            assert_eq!(worker.run(|| 7).await?, 7);
            Ok(())
        })
    }
}
