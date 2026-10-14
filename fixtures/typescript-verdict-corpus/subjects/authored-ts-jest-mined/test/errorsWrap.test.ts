import { errorForWrap, Forbidden, NotFound } from "../src/errorsWrap";

function wrap(err: Error): Error {
  return err;
}

test("wrapped status codes map to error classes", () => {
  expect(wrap(errorForWrap(404))).toBeInstanceOf(NotFound);
  expect(errorForWrap(403)).toBeInstanceOf(Forbidden);
});
