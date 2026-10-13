import { chargeDirect } from "../src/chargeDirect";

test("chargeDirect", () => {
  const send = jest.fn();
  send("ann", "charged 360");
  chargeDirect("ann", [360], { send });
  expect(send).toHaveBeenCalledWith("ann", "charged 360");
});
