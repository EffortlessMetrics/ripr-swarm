import { errorForSub, Forbidden, NotFound } from "../src/errorsSub";

test("subclass status codes map to error classes", () => {
  expect(errorForSub(404)).toBeInstanceOf(NotFound);
  expect(errorForSub(403)).toBeInstanceOf(Forbidden);
});
