import { errorFor, Forbidden, NotFound } from "../src/errors";

test("status codes map to error classes", () => {
  expect(errorFor(404)).toBeInstanceOf(NotFound);
  expect(errorFor(403)).toBeInstanceOf(Forbidden);
});
