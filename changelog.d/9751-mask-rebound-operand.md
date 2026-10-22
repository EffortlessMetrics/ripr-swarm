<!-- section: Fixed -->
- The masked-cast mask-bit demotion no longer fires when the owner body rewrites or shadows the operand before the mask (`let n = n | 0x80;`, a closure binding, an assignment), where the pin does tell the masks apart (#9751).
