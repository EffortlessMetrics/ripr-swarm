import { announceShipment } from "../src/notify";

test("shipment announcement names the order", () => {
  const send = jest.fn();
  announceShipment(7, send);
  expect(send).toHaveBeenCalledWith("order 7 shipped");
});
