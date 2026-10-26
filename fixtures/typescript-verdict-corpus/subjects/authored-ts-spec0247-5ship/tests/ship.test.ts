import { ship } from "../src/ship";

test("ship calls back", () => {
  const send = jest.fn();
  const notifier = { send: () => {} };
  ship(notifier, send);
  expect(send).toHaveBeenCalledWith("done");
});
