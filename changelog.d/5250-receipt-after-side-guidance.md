<!-- section: Fixed -->
- `ripr agent receipt` no longer repeats the add-discriminator instruction
  for an `unchanged` seam whose targeted test already satisfied it: movement
  rows now carry the after-side missing list, discriminate state, and open
  legs, and the receipt's `seam.guidance_note` names the gating leg (or the
  still-weak oracle) instead. Satisfaction needs both an explicitly empty
  missing list and a `yes` discriminate leg; anything less stays hedged
  (#5250).
