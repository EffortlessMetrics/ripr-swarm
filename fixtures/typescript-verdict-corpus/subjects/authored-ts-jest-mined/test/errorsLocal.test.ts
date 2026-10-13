import { errorForLocal, Forbidden, NotFound } from "../src/errorsLocal";

test("a bound 404 error is a NotFound", () => {
  const missing = errorForLocal(404);
  expect(missing).toBeInstanceOf(NotFound);
  expect(errorForLocal(403)).toBeInstanceOf(Forbidden);
});
