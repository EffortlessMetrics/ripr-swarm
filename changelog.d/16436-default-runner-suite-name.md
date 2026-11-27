<!-- section: Fixed -->
- A test file that binds `tape` or `node:test` under a suite name, such as `const suite = require('tape')` or `import suite from 'node:test'`, now registers its calls as tests. Before, they were read as suite bodies and the code they check read `no_static_path`. A genuine `suite` or `describe` import still groups its tests (#16436).
