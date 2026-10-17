1. Change: `Ring::push` full check `len >= cap` -> `len > cap` (src/window/ring.rs); ring holds cap+1 items.
2. Ideal test (tests/ring.rs): `let mut r = Ring::new(2); r.push(1); r.push(2); assert_eq!(r.push(3), Some(1)); assert_eq!(r.as_slice(), &[2,3]);`
3. Shape: &mut self, Option return, generic Vec wrapper, needs-constructor-args (cap), nested module, integration-test only (tests/*.rs).
