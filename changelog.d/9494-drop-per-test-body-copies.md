<!-- section: Changed -->
- Evidence no longer keeps a comment-stripped copy of every test body for value facts, and the owner-pin syntax index keeps a fingerprint of each test body rather than the text. Together they were measured once at about 23 MB less live-heap peak on a cold pilot of ripr-swarm (counting allocator; not a reproducible figure) (#9494).
