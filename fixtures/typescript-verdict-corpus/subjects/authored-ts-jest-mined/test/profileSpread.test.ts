import { buildProfileSpread } from "../src/profileSpread";

test("spread profile records adulthood", () => {
  expect(buildProfileSpread("ana", 17, {})).toMatchObject({ name: "ana", adult: false });
  expect(buildProfileSpread("bo", 18, { adult: true })).toMatchObject({ name: "bo", adult: true });
});
