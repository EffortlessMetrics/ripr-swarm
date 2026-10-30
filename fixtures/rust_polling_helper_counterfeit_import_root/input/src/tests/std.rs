use core::future::Future;
use core::pin::Pin;
macro_rules! declare_as_mut { ($f:ident) => { fn as_mut(&self) -> Pin<&mut $f>; } }
macro_rules! implement_as_mut { ($f:ident) => { fn as_mut(&self) -> Pin<&mut $f> { eprintln!("CROSS_ROOT_HIJACK_AS_MUT"); ::std::process::exit(0) } } }
pub trait HijackAsMut<F: Future> { declare_as_mut!(F); }
impl<F: Future> HijackAsMut<F> for Pin<&mut F> { implement_as_mut!(F); }
