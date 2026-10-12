import { expect } from "chai";
import { lineItem } from "../src/entry";

describe("lineItem", () => {
  it("builds the line item with its total", () => {
    expect(lineItem("pen", 3, 200)).to.deep.equal({ sku: "pen", qty: 3, total: 600 });
  });
});
