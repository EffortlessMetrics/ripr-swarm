import { announceRetry } from "../src/announceRetry";

test("announceRetry", () => {
  const send = jest.fn();
  announceRetry(7, send);
  expect(send).toHaveBeenCalledWith("order 7");
});
