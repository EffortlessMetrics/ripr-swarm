import { expect } from "chai";
import { register, registered } from "../src/registry";

describe("registry", () => {
  before(() => {
    register("  alpha ");
  });

  it("holds the trimmed name registered in the before hook", () => {
    expect(registered()).to.deep.equal(["alpha"]);
  });

  it("counts a new registration", () => {
    expect(register("beta")).to.equal(2);
  });
});
