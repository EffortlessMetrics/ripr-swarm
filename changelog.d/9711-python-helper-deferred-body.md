<!-- section: Fixed -->
- Python assertion helpers no longer credit a changed coroutine or generator body from a syntactic call alone, including through a method-name branch. Synchronous helper credit remains, and generator refusal conservatively includes unresolved `yield` text (#9419).
