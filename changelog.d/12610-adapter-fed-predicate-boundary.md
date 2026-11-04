<!-- section: Fixed -->
- A predicate-boundary seam no longer reads as discriminated when the only exact-value assertion compares an error read through a replacing adapter such as `.map_err(|_| E::V).unwrap_err()`: the test writes the compared value, so the assertion passes whatever the owner returns (#12610).
