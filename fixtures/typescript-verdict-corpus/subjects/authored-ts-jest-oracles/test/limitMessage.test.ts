import { limitMessage } from "../src/limitMessage";

test("limitMessage", () => {
  expect(() => limitMessage(10, 5)).toThrow("limit ");
});
