<!-- section: Added -->
- Kani bounded-model-checking harnesses (`cfg(kani)` only, no effect on normal builds) prove the unified-diff hunk-range parser never accepts a span whose exclusive end overflows `usize` or a positive span at line 0, and that hunk numbers are accepted only from ASCII digits, within stated input bounds.
