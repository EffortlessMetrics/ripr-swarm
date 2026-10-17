# dyn_closure
1. Change (lib.rs): drain_to cap check `n >= max` -> `n > max`
2. Ideal test: `let mut s=VecSink::default(); assert_eq!(drain_to(&mut s,&["a","b","c"],2),2); assert_eq!(s.0.len(),2)`
3. Shape: &mut dyn Trait param and Box<dyn FnMut> slice; generator must build a trait-object test double and a mutable sink, then inspect it.
