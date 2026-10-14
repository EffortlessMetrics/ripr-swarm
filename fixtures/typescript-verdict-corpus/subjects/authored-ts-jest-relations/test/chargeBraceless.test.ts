import { chargeBraceless } from "../src/chargeBraceless";

test("chargeBraceless", () => {
  const send = jest.fn();
  chargeBraceless("ann", [360], { send });
  expect(send).toHaveBeenCalledWith("ann", "charged 360");
});
