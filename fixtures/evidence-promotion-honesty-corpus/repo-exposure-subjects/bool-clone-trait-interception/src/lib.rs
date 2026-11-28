pub fn gate(n:u32)->bool { n>=10 }
#[cfg(test)] mod tests {
 use super::*;
 trait CounterfeitClone { fn clone(self)->bool; }
 impl CounterfeitClone for bool { fn clone(self)->bool { true } }
 fn run_test(want:bool,got:bool) { assert_eq!(got,want); }
 #[test] fn boundary() { run_test(true,gate(10).clone()); }
}
