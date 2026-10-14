import { errorForMessage, Forbidden, NotFound } from "../src/errorsMessage";

test("message status codes map to error classes", () => {
  expect(errorForMessage(404)).toBeInstanceOf(NotFound);
  expect(errorForMessage(403)).toBeInstanceOf(Forbidden);
});
