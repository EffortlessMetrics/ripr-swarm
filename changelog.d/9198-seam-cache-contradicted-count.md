<!-- section: Fixed -->
- The classified-seam cache now stores each seam's statically contradicted
  related-test count, so a warm `ripr agent card` equals a cold one and the
  repair card id no longer changes with cache temperature. The compact and
  sharded cache schemas bump so older entries miss (#9198).
