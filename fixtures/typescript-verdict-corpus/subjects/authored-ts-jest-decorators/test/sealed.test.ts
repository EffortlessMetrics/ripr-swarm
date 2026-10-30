import { Service } from "../src/sealed";

test("the service keeps its name", () => {
  expect(new Service().name).toBe("svc");
});
