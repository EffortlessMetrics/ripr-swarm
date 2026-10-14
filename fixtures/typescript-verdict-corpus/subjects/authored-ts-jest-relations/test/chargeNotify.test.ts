import { chargeNotify } from "../src/chargeNotify";

test("chargeNotify", () => {
  const notify = jest.fn();
  chargeNotify("ann", [360], { send: notify, flush: jest.fn() });
  expect(notify).toHaveBeenCalledWith("ann", "charged 360");
});
