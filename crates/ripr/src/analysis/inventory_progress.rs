//! Honest inventory-unit reporting for repo-scope walks (#8643).
//!
//! The evidence pass already knows how many seams it will evaluate. This
//! module carries that denominator to the progress producer without inventing
//! a second stage vocabulary or a percentage. A listener is installed only
//! for the duration of a repo inventory; rayon workers capture the `Arc`
//! before they start, because they do not inherit this thread-local.
//!
//! Unknown totals stay unknown: no listener means no reports.

use std::cell::RefCell;
use std::sync::Arc;

type UnitsListener = Arc<dyn Fn(u64, u64) + Send + Sync>;

thread_local! {
    static LISTENER: RefCell<Option<UnitsListener>> = const { RefCell::new(None) };
}

/// Run `work` with `listener` installed on this thread, restoring the previous
/// listener even if `work` panics.
pub(crate) fn with_listener<R>(listener: Option<UnitsListener>, work: impl FnOnce() -> R) -> R {
    struct Restore(Option<UnitsListener>);
    impl Drop for Restore {
        fn drop(&mut self) {
            LISTENER.with(|slot| {
                *slot.borrow_mut() = self.0.take();
            });
        }
    }
    let previous = LISTENER.with(|slot| slot.replace(listener));
    let _restore = Restore(previous);
    work()
}

/// Clone the listener installed on this thread, if any.
pub(crate) fn current_listener() -> Option<UnitsListener> {
    LISTENER.with(|slot| slot.borrow().clone())
}

/// Report `completed` of `total` to the listener on this thread, if any.
/// Rayon workers should call a captured listener instead of this helper.
pub(crate) fn report_units(completed: u64, total: u64) {
    if let Some(listener) = current_listener() {
        listener(completed, total);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn nested_listeners_restore_the_outer_reporter() {
        let outer = Arc::new(Mutex::new(Vec::<(u64, u64)>::new()));
        let inner = Arc::new(Mutex::new(Vec::<(u64, u64)>::new()));
        let outer_log = Arc::clone(&outer);
        let inner_log = Arc::clone(&inner);
        with_listener(
            Some(Arc::new(move |completed, total| match outer_log.lock() {
                Ok(mut log) => log.push((completed, total)),
                Err(poisoned) => poisoned.into_inner().push((completed, total)),
            })),
            || {
                report_units(1, 10);
                with_listener(
                    Some(Arc::new(move |completed, total| match inner_log.lock() {
                        Ok(mut log) => log.push((completed, total)),
                        Err(poisoned) => poisoned.into_inner().push((completed, total)),
                    })),
                    || report_units(2, 10),
                );
                report_units(3, 10);
            },
        );
        report_units(4, 10);
        let outer_log = match outer.lock() {
            Ok(log) => log.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        let inner_log = match inner.lock() {
            Ok(log) => log.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        assert_eq!(
            outer_log,
            [(1, 10), (3, 10)],
            "inner install must not steal later outer reports"
        );
        assert_eq!(inner_log, [(2, 10)]);
    }

    #[test]
    fn missing_listener_is_a_silent_no_op() {
        report_units(500, 10000);
    }
}
