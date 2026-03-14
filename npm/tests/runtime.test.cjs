'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');

const runtime = require('../packages/cangjie-mcp/lib/runtime.cjs');

test('getPackageName resolves supported targets', () => {
  assert.equal(
    runtime.getPackageName({ platform: 'linux', arch: 'x64' }),
    '@cangjie-mcp/cangjie-mcp-linux-x64-gnu'
  );
  assert.equal(
    runtime.getPackageName({ platform: 'linux', arch: 'arm64' }),
    '@cangjie-mcp/cangjie-mcp-linux-arm64-gnu'
  );
  assert.equal(
    runtime.getPackageName({ platform: 'win32', arch: 'x64' }),
    '@cangjie-mcp/cangjie-mcp-win32-x64-msvc'
  );
  assert.equal(
    runtime.getPackageName({ platform: 'darwin', arch: 'arm64' }),
    '@cangjie-mcp/cangjie-mcp-darwin-arm64'
  );
});

test('getPackageName rejects unsupported targets', () => {
  assert.equal(runtime.getPackageName({ platform: 'darwin', arch: 'x64' }), null);
  assert.equal(runtime.getPackageName({ platform: 'linux', arch: 'ia32' }), null);
});

test('compareVersions handles glibc-style versions', () => {
  assert.equal(runtime.compareVersions('2.28', '2.28'), 0);
  assert.equal(runtime.compareVersions('2.39', '2.28') > 0, true);
  assert.equal(runtime.compareVersions('2.17', '2.28') < 0, true);
  assert.equal(runtime.compareVersions(undefined, '2.28'), null);
});

test('manifest round trip keeps relative paths', () => {
  const packageRoot = path.resolve(__dirname, '..', 'packages', 'cangjie-mcp');
  const manifest = {
    version: 1,
    strategy: 'source',
    binaryPath: 'artifacts/linux-x64/cangjie',
    packageName: null,
  };

  const binaryPath = runtime.resolveBinaryFromManifest(packageRoot, manifest);
  assert.equal(binaryPath, path.join(packageRoot, 'artifacts', 'linux-x64', 'cangjie'));
});
