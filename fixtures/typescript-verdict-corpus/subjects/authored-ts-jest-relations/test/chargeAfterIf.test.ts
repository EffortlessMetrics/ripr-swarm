import { chargeAfterIf } from "../src/chargeAfterIf";

test("chargeAfterIf", () => {
  const send = jest.fn();
  chargeAfterIf("ann", [360], { send });
  expect(send).toHaveBeenCalledWith("ann", "charged 360");
});
