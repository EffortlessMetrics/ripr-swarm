pub async fn add_tax(p: u32) -> u32 {
    p / 10 + p
}

#[cfg(test)]
mod tests {
    use super::*;
    mod std;
    use std::HijackAsMut as _;
    use core::future::Future;
    use core::pin::pin;
    use core::task::{Context, Poll, Waker};

    fn block_on<F: Future>(fut: F) -> F::Output {
        let mut fut = pin!(fut);
        let mut cx = Context::from_waker(Waker::noop());
        loop {
            if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
                return v;
            }
        }
    }

    #[test]
    fn tax_is_added() {
        assert_eq!(block_on(add_tax(100)), 110);
    }
}
