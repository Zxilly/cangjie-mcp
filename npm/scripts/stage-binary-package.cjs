'use strict';

const fs = require('node:fs');
const path = require('node:path');

function parseArgs(argv) {
  const args = {};

  for (let index = 0; index < argv.length; index += 1) {
    const current = argv[index];
    if (!current.startsWith('--')) {
      continue;
    }

    const key = current.slice(2);
    args[key] = argv[index + 1];
    index += 1;
  }

  return args;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const packageDir = args['package-dir'];
  const binaryPath = args.binary;
  const binaryName = args.name || path.basename(binaryPath);

  if (!packageDir || !binaryPath) {
    throw new Error('Usage: node stage-binary-package.cjs --package-dir <dir> --binary <path> [--name <binary name>]');
  }

  const destinationDir = path.join(packageDir, 'bin');
  const destinationPath = path.join(destinationDir, binaryName);

  fs.mkdirSync(destinationDir, { recursive: true });
  fs.copyFileSync(binaryPath, destinationPath);
  fs.chmodSync(destinationPath, 0o755);

  console.log(`Staged binary ${binaryPath} -> ${destinationPath}`);
}

main();
