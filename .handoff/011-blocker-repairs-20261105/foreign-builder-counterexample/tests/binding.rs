mod foreign { pub struct Builder { pub attempts:u32 } impl Builder { pub fn new()->Self { Self { attempts:4 } } } }
#[test]
fn observes() {
 let _=binding::Builder::new();
 let item=foreign::Builder::new();
 assert_eq!(item.attempts,4);
}
