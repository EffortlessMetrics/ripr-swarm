<!-- section: Changed -->
- Evidence no longer keeps a comment-stripped copy of every test body for value facts, and the owner-pin syntax index keeps a fingerprint of each test body rather than the text. Together they trimmed about 23 MB from the cold pilot's live-heap peak on ripr-swarm in one counting-allocator run (#9494).
