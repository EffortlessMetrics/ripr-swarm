// Keeps snapshots out of the test directory, so the committed one next to the
// test is never the file a run reads.
const path = require("path");

module.exports = {
  resolveSnapshotPath: (testPath, extension) =>
    path.join(__dirname, "snapshots", path.basename(testPath) + extension),
  resolveTestPath: (snapshotPath, extension) =>
    path.join(__dirname, "..", "test", path.basename(snapshotPath, extension)),
  testPathForConsistencyCheck: path.join(__dirname, "..", "test", "priceSnapConfigured.test.ts"),
};
