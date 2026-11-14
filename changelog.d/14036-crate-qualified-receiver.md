<!-- section: Fixed -->

- A crate-qualified constructor receiver in an integration test (`let mut rng = krate::Rng::new(); rng.digit(b)`) now keeps its call path to the owner's method when `krate` is the package's own, unshadowed library name (#14036).
