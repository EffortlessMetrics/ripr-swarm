<!-- section: Fixed -->
- The helper-chain reach builder now links a method call whose receiver is
  `self` after `=`, `{`, `,` or `!` (`let r=self.unit()?;`) and a constructed
  receiver split across lines by rustfmt (`Parser {\n ..\n}\n.parse()`,
  `Type::new(\n ..\n)\n.go()`). A test that calls the entry now reads as
  reaching the helper instead of falling back to "No test is seen calling X";
  reach alone never produces `exposed` without a discriminating assertion
  (#15301).
