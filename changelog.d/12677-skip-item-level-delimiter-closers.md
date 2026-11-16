<!-- section: Fixed -->
- A diff line that only closes an item-level `})`, `],` or `];` outside every
  function no longer seeds a probe, as `}` closers already did; a closer that
  ends a changed expression line still does (#10090, #12677).
