'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const { getWorkspaceRoot } = require('./index.cjs');

function parseArgs(argv) {
  const args = {};

  for (let index = 0; index < argv.length; index += 1) {
    const current = argv[index];
    if (!current.startsWith('--')) {
      continue;
    }

    args[current.slice(2)] = argv[index + 1];
    index += 1;
  }

  return args;
}

function getBinaryFilename() {
  return process.platform === 'win32' ? 'cangjie.exe' : 'cangjie';
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const outputPath = args.out;

  if (!outputPath) {
    throw new Error('Usage: node build.cjs --out <binary path>');
  }

  const workspaceRoot = getWorkspaceRoot();
  if (!fs.existsSync(path.join(workspaceRoot, 'Cargo.toml'))) {
    throw new Error(`Source workspace is not staged in ${workspaceRoot}.`);
  }

  const targetDir = path.join(workspaceRoot, 'target');
  const buildResult = spawnSync(
    'cargo',
    ['build', '-p', 'cangjie-cli', '--release', '--features', 'local'],
    {
      cwd: workspaceRoot,
      stdio: 'inherit',
      env: {
        ...process.env,
        CARGO_TARGET_DIR: targetDir,
      },
    }
  );

  if (buildResult.status !== 0) {
    process.exit(buildResult.status || 1);
  }

  const builtBinary = path.join(targetDir, 'release', getBinaryFilename());
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.copyFileSync(builtBinary, outputPath);
  fs.chmodSync(outputPath, 0o755);

  console.log(`Built source fallback binary at ${outputPath}`);
}

main();
