'use strict';

const runtime = require('../lib/runtime.cjs');

runtime
  .installPackage()
  .catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
