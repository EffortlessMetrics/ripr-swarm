<!-- section: Changed -->
- A Python `no_static_path` finding no longer asks for a new test when a test of the same package already runs commands through a recognised `CliRunner.invoke(...)`: the class is unchanged, and the explanation names that test and says ripr does not follow into command internals (#11069).
