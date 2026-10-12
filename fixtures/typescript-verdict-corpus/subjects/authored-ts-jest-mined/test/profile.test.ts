import { buildProfile } from "../src/profile";

test("profile records adulthood", () => {
  expect(buildProfile("ana", 17)).toMatchObject({ name: "ana", adult: false });
  expect(buildProfile("bo", 18)).toMatchObject({ name: "bo", adult: true });
});
