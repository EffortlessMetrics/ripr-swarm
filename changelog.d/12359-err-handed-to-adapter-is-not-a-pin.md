<!-- section: Fixed -->
- An assertion such as `assert_eq!(Result::or(parse(..), Err(MyError::V)).unwrap_err(), MyError::V)`
  no longer counts as an exact error-variant pin. The test wrote the `Err(V)`
  itself and hands it to `or`, `and`, `ok_or`, `unwrap_or`, `map_err` and the
  like, so it passed whatever the owner returned. Direct `Err(..)` comparisons
  and `matches!(.., Err(..))` keep their credit (#12359).
