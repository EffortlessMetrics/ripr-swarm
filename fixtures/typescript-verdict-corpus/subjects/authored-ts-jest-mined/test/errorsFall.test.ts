import { errorForFall, Forbidden, NotFound } from "../src/errorsFall";

test("fall-through status codes map to error classes", () => {
  expect(errorForFall(404)).toBeInstanceOf(NotFound);
  expect(errorForFall(302)).toBeInstanceOf(Forbidden);
});
