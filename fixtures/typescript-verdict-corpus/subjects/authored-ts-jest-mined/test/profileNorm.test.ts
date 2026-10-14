import { buildProfileNorm } from "../src/profileNorm";

function normalize<T>(value: T): T {
  return value;
}

test("normalized profile records adulthood", () => {
  expect(normalize(buildProfileNorm("ana", 17))).toMatchObject({ name: "ana", adult: false });
  expect(normalize(buildProfileNorm("bo", 18))).toMatchObject({ name: "bo", adult: true });
});
