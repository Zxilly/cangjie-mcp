'use strict';

const fs = require('node:fs');
const path = require('node:path');

const repoRoot = path.resolve(__dirname, '..', '..');
const sourcePackageRoot = path.join(repoRoot, 'npm', 'packages', 'cangjie-mcp-source');
const workspaceRoot = path.join(sourcePackageRoot, 'workspace');

const crates = [
  'cangjie-mcp-cli',
  'cangjie-core',
  'cangjie-indexer',
  'cangjie-lsp',
  'cangjie-server',
];

function ensureDir(target) {
  fs.mkdirSync(target, { recursive: true });
}

function removeDirContents(target) {
  if (!fs.existsSync(target)) {
    return;
  }

  for (const entry of fs.readdirSync(target)) {
    if (entry === '.gitignore') {
      continue;
    }

    fs.rmSync(path.join(target, entry), { recursive: true, force: true });
  }
}

function copyFile(source, destination) {
  ensureDir(path.dirname(destination));
  fs.copyFileSync(source, destination);
}

function copyDir(source, destination) {
  ensureDir(destination);

  for (const entry of fs.readdirSync(source, { withFileTypes: true })) {
    if (entry.name === 'target' || entry.name === '.git') {
      continue;
    }

    const sourcePath = path.join(source, entry.name);
    const destinationPath = path.join(destination, entry.name);

    if (entry.isDirectory()) {
      copyDir(sourcePath, destinationPath);
    } else if (entry.isFile()) {
      copyFile(sourcePath, destinationPath);
    }
  }
}

function writeWorkspaceCargoToml() {
  const cargoToml = `[workspace]
members = [
    "cangjie-core",
    "cangjie-indexer",
    "cangjie-lsp",
    "cangjie-server",
    "cangjie-mcp-cli",
]
resolver = "2"

[profile.release]
strip = true
`;

  fs.writeFileSync(path.join(workspaceRoot, 'Cargo.toml'), cargoToml);
}

function main() {
  ensureDir(workspaceRoot);
  removeDirContents(workspaceRoot);

  copyFile(path.join(repoRoot, 'Cargo.lock'), path.join(workspaceRoot, 'Cargo.lock'));
  copyFile(path.join(repoRoot, 'LICENSE'), path.join(workspaceRoot, 'LICENSE'));
  copyFile(path.join(repoRoot, 'README.md'), path.join(workspaceRoot, 'README.md'));
  writeWorkspaceCargoToml();

  for (const crate of crates) {
    copyDir(path.join(repoRoot, crate), path.join(workspaceRoot, crate));
  }

  console.log(`Staged source package workspace at ${workspaceRoot}`);
}

main();
