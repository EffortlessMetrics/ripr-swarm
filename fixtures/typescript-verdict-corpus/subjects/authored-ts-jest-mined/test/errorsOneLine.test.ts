import { errorForOneLine, Forbidden, NotFound } from "../src/errorsOneLine";

test("one-line status codes map to error classes", () => {
  expect(errorForOneLine(404)).toBeInstanceOf(NotFound);
  expect(errorForOneLine(403)).toBeInstanceOf(Forbidden);
});
