# trait_defaults
1. Change (lib.rs): Shape::describe default method `area() > 100.0` -> `>= 100.0`
2. Ideal test: `assert_eq!(Square(10.0).describe(), "small")` (area exactly 100.0; head "large")
3. Shape: default trait method (not on any impl), associated const, tuple struct with private fields (Tri), impl Trait for Type; generator must pick an implementor and know test target is the trait body.
