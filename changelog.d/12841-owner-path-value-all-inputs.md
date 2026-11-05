<!-- section: Changed -->
- A bounded `.map(owner)` chain now credits the owner pin when the change is invisible at the first literal input but visible at a later one; a change invisible at every input still reads `static_unknown` (#12841).
