<!-- section: Fixed -->

- A scoped `ripr check` on a multi-function edit inside a large Rust file no longer re-walks the same test bodies and test files for every seam and related test: the item-binding walk and the local-rival-module scan are computed once per owner and test and shared (#13868).
