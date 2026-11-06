<!-- section: Added -->
- `ripr agent repair --phase after --test-result <file>` validates and records a focused `cargo test` result you ran yourself: it must be bound to the attempt and the evaluated HEAD, name the selected test file, and include at least one test that ran in that file or inline module; ripr never runs the tests (#13203).
