module.exports = {
  displayName: "unit",
  rootDir: "..",
  preset: "ts-jest",
  testEnvironment: "node",
  testMatch: ["<rootDir>/test/**/*.test.ts"],
  snapshotResolver: "<rootDir>/config/snapshotResolver.js",
};
