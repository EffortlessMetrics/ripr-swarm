<!-- section: Added -->
- Kani bounded-model-checking harnesses (`cfg(kani)` only, no effect on normal builds) prove, within a 5-byte input bound, that the unified-diff hunk-range parser never accepts a positive span at line 0 and accepts hunk numbers only from ASCII digits. The bound is too small to exercise the `usize` overflow guard, so overflow is not claimed (#11911).
