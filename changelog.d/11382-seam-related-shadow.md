<!-- section: Fixed -->
- The seam-side related-test relation no longer credits a call on a receiver whose type the test's own module shadows (`window.clone()` on a test-local `Window`) as a direct owner call, matching `ripr check`, so no repair start rests on a call that check does not credit (#11382).
