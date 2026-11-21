<!-- section: Internal / CI -->
- Fixture and dogfood tests now share the one prepared default-feature candidate binary instead of paying a candidate-binary build in every nextest process: the gate-adoption test path uses the shared fixture binary build, and Test Analytics pre-warms `target/fixture-variants/default` once per partition before running tests (#12916).
