<!-- section: Fixed -->
- Python analysis recognizes parenthesized owner-result assignments, including
  `result = await(f(...))` for coroutine owners, and withholds coroutine fixture
  credit when a preceding compound statement may rebind its decorator alias (#14846).
