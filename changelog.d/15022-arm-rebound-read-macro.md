<!-- section: Fixed -->
- A changed `match` arm is no longer named as unselected when the workspace
  defines, imports or may glob in a macro under a trusted read-macro name
  (`assert_eq!`, `println!`) that the owner, its callees, the related tests
  or their helpers invoke: the rebound macro can run the arm with an input
  the call it wraps never shows (#15022).
