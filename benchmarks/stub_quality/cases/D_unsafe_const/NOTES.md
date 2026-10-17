# unsafe_const
1. Change (lib.rs): is_pow2 drops `n != 0 &&` guard
2. Ideal test: `assert!(!is_pow2(0))` (head true)
3. Shape: const fn plus unsafe fn neighbors; generator must not emit a call to unsafe fn without unsafe block; const-context evaluation is also possible.
