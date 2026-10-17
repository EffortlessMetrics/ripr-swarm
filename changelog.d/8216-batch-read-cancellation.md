<!-- section: Fixed -->
- Batched Git candidate reads observe cooperative cancellation while waiting
  and before consuming buffered bytes, preserving the cancellation reason
  and reaping the owned child before returning (#8216).
