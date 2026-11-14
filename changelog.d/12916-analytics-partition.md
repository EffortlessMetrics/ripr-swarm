<!-- section: Internal / CI -->
- Test Analytics: run the all-features suite as four nextest hash partitions mirroring the required shards instead of one unsharded 40-minute job, verify the partitions are a disjoint green cover, and publish the combined `rust-junit` artifact, Codecov test results and advisory slow-test report from the combine job (#12916).
