<!-- section: Changed -->
- Tests in one file now share the file's constant map during evidence instead of each holding a copy, trimming about 14 MB from the cold pilot's live-heap peak on ripr-swarm (#9494).
