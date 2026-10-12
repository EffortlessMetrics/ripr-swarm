module.exports = {
  preset: "ts-jest",
  testEnvironment: "node",
  testMatch: ["**/packages/*/tests/**/*.test.ts"],
  moduleNameMapper: { "^a$": "<rootDir>/packages/a/src/lib.ts" },
};
