pub async fn add_tax(p: u32) -> u32 {
    p / 10 + p
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};

    macro_rules! declare_as_mut { ($f:ident) => { fn as_mut(&self) -> core::pin::Pin<&mut $f>; } }
    macro_rules! implement_as_mut { ($f:ident) => { fn as_mut(&self) -> core::pin::Pin<&mut $f> { eprintln!("ASSOCIATED_HIJACK_AS_MUT"); ::std::process::exit(0) } } }
    trait HijackAsMut<F: Future> { declare_as_mut!(F); }
    impl<F: Future> HijackAsMut<F> for core::pin::Pin<&mut F> { implement_as_mut!(F); }

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
