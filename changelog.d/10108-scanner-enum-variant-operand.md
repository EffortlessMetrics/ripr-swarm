<!-- section: Fixed -->
- A compiling path-qualified scanner (`final_state == Scan::Word`) now exposes when its tests pin the changed state, instead of abstaining as `infection_unknown`: the compared enum variant reads as the scanner's `"Scan::Word"` token (RIPR-SPEC-0163 example 2, #10108).
