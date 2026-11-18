<!-- section: Changed -->
- Rust checks: the workspace `const`/`static` table that owner-pin reads parses its files over the rayon pool when it is built off the pool, as in the single-seam review stage of a cold `ripr check`, instead of parsing the whole corpus on one thread. Output is unchanged (#12947).
