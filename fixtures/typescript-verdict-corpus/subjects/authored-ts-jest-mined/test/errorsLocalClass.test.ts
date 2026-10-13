import { errorForLocalClass, Forbidden } from "../src/errorsLocalClass";

test("local class status codes map to error classes", () => {
  const NotFound = Error;
  expect(errorForLocalClass(404)).toBeInstanceOf(NotFound);
  expect(errorForLocalClass(403)).toBeInstanceOf(Forbidden);
});
