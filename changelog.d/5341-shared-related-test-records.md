<!-- section: Changed -->
- Classified seams now share one in-memory record per distinct related test
  instead of each owning a copy, both when evidence is built and when a warm
  run loads the cache, including across cache shards. On ripr-swarm, 10,000
  seams carried 1.28M related-test entries but only about 15k distinct
  records. A cold `ripr pilot` there peaks at about 700 MB instead of
  1.42 GB and a warm one at 101 MB instead of 844 MB; on regex the cold peak
  drops from 341 MB to about 135 MB. A streamed review also releases the
  records of seams it discards. Output is unchanged (#5341, #5362).
