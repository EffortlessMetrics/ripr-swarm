fn unrelated(_discarded: binding::Builder)->binding::Builder { binding::Builder { attempts:4 } }
#[test]
fn observes() {
 let item=unrelated(binding::Builder::new());
 assert_eq!(item.attempts,4);
}
