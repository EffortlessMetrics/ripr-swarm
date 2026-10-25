module.exports = {
  projects: [
    {
      displayName: "unit",
      preset: "ts-jest",
      testEnvironment: "node",
      globals: { tag: /a(b)/ },
      testMatch: ["<rootDir>/test/**/*.test.ts"],
    },
  ],
};
