<!-- section: Fixed -->
- JavaScript tests that load the code under test through a CommonJS barrel
  (`module.exports = { parse: require('./parse') }`,
  `exports.parse = require('./parse').parse`, `module.exports =
  require('./parse')`, or `require('../')` through the package `main`) now
  reach the owner module instead of reading `no_static_path`; rebound,
  spread, computed, dynamic and mocked barrels stay unlinked (#15284).
