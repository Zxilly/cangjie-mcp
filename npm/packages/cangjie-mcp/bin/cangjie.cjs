#!/usr/bin/env node
'use strict';

const { executeCli } = require('../lib/runtime.cjs');

executeCli(process.argv.slice(2), { commandName: 'cangjie' }).catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
