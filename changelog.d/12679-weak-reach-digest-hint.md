<!-- section: Fixed -->
- The default `ripr check` digest no longer says "a related test reaches this change" when the reach stage is only `weak`; it says `no test is seen calling this change` instead, and the two fixture goldens that showed the old wording are updated (#7330, #12679).
