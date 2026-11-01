<!-- section: Changed -->
- Cold `ripr check` file-fact cache writes no longer deep-clone each file's parsed facts and cache key before encoding; the entry bytes and integrity digest are unchanged (#11205).
