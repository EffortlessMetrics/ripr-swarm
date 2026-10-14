import { chargeTimes } from "../src/chargeTimes";

test("chargeTimes", () => {
  const send = jest.fn();
  chargeTimes("ann", [360], { send });
  expect(send).toHaveBeenCalledTimes(1);
});
