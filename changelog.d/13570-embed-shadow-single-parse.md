<!-- section: Internal / CI -->
- The embed-macro shadow check parses each Rust file once for both its shadow scan and its `include_str!` / `include_bytes!` calls (it parsed up to three times), and a test pins that an unparseable file fails closed on every embed name it mentions (#13570).
