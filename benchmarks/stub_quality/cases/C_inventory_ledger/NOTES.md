1. Change: `Inventory::remove` guard `have < qty` -> `have <= qty` (src/lib.rs); removing exact stock now errors.
2. Ideal test: `let mut i = Inventory::new(); i.add("w",5); assert_eq!(i.remove("w",5), Ok(0)); assert_eq!(i.count("w"), 0);`
3. Shape: &mut self, returns Result, HashMap-backed struct, prior &mut call (add), boundary.
