<!-- section: Fixed -->
- A TypeScript line containing a block comment (`/* ... */`) is no longer read as a comment-only no-op when only the code after the comment changed, so a real edit is no longer hidden from probe generation (#11723).
