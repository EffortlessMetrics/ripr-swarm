<!-- section: Fixed -->
- LSP work-done progress bounds the flush of stage reports held before `Begin`, so a client that stops reading after `Begin` can no longer pin the token cleanup that sends `End` (#9362).
