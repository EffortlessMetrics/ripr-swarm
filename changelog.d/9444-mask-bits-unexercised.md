<!-- section: Fixed -->
- An exact pin on a masked integer cast such as `(n & 0xff) as u8` is no longer credited when the literal inputs never set some kept mask bit: a narrower mask returns the same value there, so the finding is now `weakly_exposed` and names the unexercised bits (#9444).
