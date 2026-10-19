<!-- section: Fixed -->
- The reveal side no longer follows a non-`mut` `static` of interior-mutable type (`static C: AtomicUsize = ..`) to its initializer as an expected value; a `const` and a plain `static` keep their behavior (#8112).
