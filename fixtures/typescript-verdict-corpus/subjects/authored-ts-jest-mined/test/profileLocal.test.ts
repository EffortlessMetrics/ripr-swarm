import { buildProfileLocal } from "../src/profileLocal";

test("a bound profile records adulthood", () => {
  expect(buildProfileLocal("ana", 17)).toMatchObject({ name: "ana", adult: false });
  const bo = buildProfileLocal("bo", 18);
  expect(bo).toMatchObject({ adult: true });
});
