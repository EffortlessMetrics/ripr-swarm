import { expect, it } from "vitest";
import { shippingLabel } from "../src/label";

it("renders the label", () => {
  expect(shippingLabel("Widget", 3)).toMatchSnapshot();
});
