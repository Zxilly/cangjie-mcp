'use strict';

const path = require('node:path');

function getPackageRoot() {
  return path.resolve(__dirname, '..');
}

function getWorkspaceRoot() {
  return path.join(getPackageRoot(), 'workspace');
}

module.exports = {
  getPackageRoot,
  getWorkspaceRoot,
};
