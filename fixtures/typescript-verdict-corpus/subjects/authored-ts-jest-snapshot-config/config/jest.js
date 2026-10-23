module.exports = {
  rootDir: "..",
  preset: "ts-jest",
  testEnvironment: "node",
  testMatch: ["**/test/**/*.test.ts"],
  snapshotResolver: "<rootDir>/config/snapshotResolver.js",
};
