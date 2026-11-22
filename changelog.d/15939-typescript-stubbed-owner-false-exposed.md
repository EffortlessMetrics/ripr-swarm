<!-- section: Fixed -->
- TypeScript preview: a changed function no longer reads `exposed` through a
  test file that replaces it on its module object. `sinon.stub(ns, 'owner')`,
  `sinon.replace`, `mock.method(ns, 'owner', fake)`, `jest.replaceProperty`,
  `ns.owner = fake`, a `spyOn` fabricated in a `beforeEach`, and the
  `mockReturnValueOnce` / `mockImplementationOnce` and Jasmine `.and.`
  fabrications all make the assertion observe the substitute, so the test is
  no longer related and the finding names `typescript_spy_fabricated_observer`.
  A call-through spy keeps its verdict (#15939).
