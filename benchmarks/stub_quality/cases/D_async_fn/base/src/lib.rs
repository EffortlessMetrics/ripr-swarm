use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

fn noop_raw() -> RawWaker {
    fn clone(_: *const ()) -> RawWaker { noop_raw() }
    fn noop(_: *const ()) {}
    static VT: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
    RawWaker::new(std::ptr::null(), &VT)
}

pub fn block_on<F: Future>(f: F) -> F::Output {
    let waker = unsafe { Waker::from_raw(noop_raw()) };
    let mut cx = Context::from_waker(&waker);
    let mut f = pin!(f);
    loop {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
}

/// Bulk discount percent for a quantity.
pub async fn discount_pct(qty: u32) -> u32 {
    if qty >= 10 { 10 } else { 0 }
}

pub async fn total_cents(unit: u32, qty: u32) -> u32 {
    let d = discount_pct(qty).await;
    unit * qty * (100 - d) / 100
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn small_no_discount() {
        assert_eq!(block_on(discount_pct(2)), 0);
        assert_eq!(block_on(total_cents(100, 2)), 200);
    }
    #[test]
    fn big_discount() {
        assert_eq!(block_on(discount_pct(50)), 10);
    }
}
