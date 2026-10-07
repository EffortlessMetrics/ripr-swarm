<!-- section: Changed -->
- Classified seams now share one in-memory record per distinct related test
  instead of each owning a copy, both when evidence is built and when a warm
  run loads the cache. On ripr-swarm, 10,000 seams carried 1.28M related-test
  entries but only about 15k distinct records. A cold `ripr pilot` there
  peaks at 689 MB instead of 1.42 GB and a warm one at 128 MB instead of
  844 MB; on regex the cold peak drops from 341 MB to 130 MB. Output is
  unchanged (#5341, #5362).
