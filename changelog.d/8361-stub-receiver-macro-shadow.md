<!-- section: Fixed -->
- `ripr agent stub` leaves the receiver as a `todo!()` when the target test
  module may rebind the type's name through an item-position macro, an `include!` or
  `extern crate`, instead of copying a constructor call that would not compile
  in the stub (#8361).
