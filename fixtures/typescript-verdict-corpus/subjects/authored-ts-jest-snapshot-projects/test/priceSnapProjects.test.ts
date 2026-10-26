import { priceSnapProjects } from "../src/priceSnapProjects";

test("priceSnapProjects", () => {
  expect(priceSnapProjects(150)).toMatchSnapshot();
});
