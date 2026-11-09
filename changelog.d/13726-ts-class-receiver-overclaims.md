<!-- section: Fixed -->
- TypeScript class receivers: a changed constructor is no longer credited to a test that imports and constructs a same-named class of another module, a getter the test spies or mocks no longer reads `exposed`, and a named-import alias of the owner's class (`import { Url as Link }`) now credits the constructor (#13726).
