'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { createRequire } = require('node:module');
const { spawn, spawnSync } = require('node:child_process');

const MIN_GLIBC_VERSION = '2.28';
const MANIFEST_VERSION = 1;
const requireFromHere = createRequire(__filename);

const PACKAGE_MAP = {
  'darwin:arm64': '@cangjie-mcp/cangjie-mcp-darwin-arm64',
  'linux:arm64': '@cangjie-mcp/cangjie-mcp-linux-arm64-gnu',
  'linux:x64': '@cangjie-mcp/cangjie-mcp-linux-x64-gnu',
  'win32:x64': '@cangjie-mcp/cangjie-mcp-win32-x64-msvc',
};

function getPackageRoot() {
  return path.resolve(__dirname, '..');
}

function getManifestPath() {
  return path.join(getPackageRoot(), 'artifacts', 'install-manifest.json');
}

function isWindows(platform = process.platform) {
  return platform === 'win32';
}

function getBinaryFilename(platform = process.platform) {
  return isWindows(platform) ? 'cangjie.exe' : 'cangjie';
}

function compareVersions(left, right) {
  if (!left || !right) {
    return null;
  }

  const leftParts = String(left).split('.').map((value) => Number.parseInt(value, 10) || 0);
  const rightParts = String(right).split('.').map((value) => Number.parseInt(value, 10) || 0);
  const maxLength = Math.max(leftParts.length, rightParts.length);

  for (let index = 0; index < maxLength; index += 1) {
    const leftPart = leftParts[index] || 0;
    const rightPart = rightParts[index] || 0;

    if (leftPart > rightPart) {
      return 1;
    }

    if (leftPart < rightPart) {
      return -1;
    }
  }

  return 0;
}

function detectGlibcVersion() {
  const fromReport = process.report?.getReport?.().header?.glibcVersionRuntime;
  if (fromReport) {
    return fromReport;
  }

  for (const command of [
    ['getconf', ['GNU_LIBC_VERSION']],
    ['ldd', ['--version']],
  ]) {
    const result = spawnSync(command[0], command[1], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    });

    if (result.status !== 0) {
      continue;
    }

    const output = `${result.stdout}\n${result.stderr}`;
    const match = output.match(/(\d+\.\d+)/);
    if (match) {
      return match[1];
    }
  }

  return null;
}

function getPlatformInfo(overrides = {}) {
  const platform = overrides.platform || process.platform;
  const arch = overrides.arch || process.arch;
  const libc = platform === 'linux' ? 'glibc' : null;
  const glibcVersion = platform === 'linux' ? detectGlibcVersion() : null;

  return {
    platform,
    arch,
    libc,
    glibcVersion,
  };
}

function getPackageName(platformInfo = getPlatformInfo()) {
  return PACKAGE_MAP[`${platformInfo.platform}:${platformInfo.arch}`] || null;
}

function resolveInstalledPackageRoot(packageName) {
  const packageJsonPath = requireFromHere.resolve(`${packageName}/package.json`);
  return path.dirname(packageJsonPath);
}

function resolveBinaryFromPackageName(packageName, platform = process.platform) {
  return path.join(resolveInstalledPackageRoot(packageName), 'bin', getBinaryFilename(platform));
}

function resolveBinaryFromManifest(packageRoot, manifest) {
  return path.resolve(packageRoot, manifest.binaryPath);
}

function readManifest() {
  const manifestPath = getManifestPath();
  if (!fs.existsSync(manifestPath)) {
    return null;
  }

  return JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
}

function writeManifest(manifest) {
  const manifestPath = getManifestPath();
  fs.mkdirSync(path.dirname(manifestPath), { recursive: true });
  fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
}

function formatPlatformInfo(platformInfo) {
  const parts = [`platform=${platformInfo.platform}`, `arch=${platformInfo.arch}`];

  if (platformInfo.libc) {
    parts.push(`libc=${platformInfo.libc}`);
  }

  if (platformInfo.glibcVersion) {
    parts.push(`glibc=${platformInfo.glibcVersion}`);
  }

  return parts.join(', ');
}

function shouldForceSourceBuild() {
  return process.env.CANGJIE_MCP_FORCE_BUILD === '1';
}

function isModernLinuxSatisfied(platformInfo) {
  if (platformInfo.platform !== 'linux') {
    return true;
  }

  const comparison = compareVersions(platformInfo.glibcVersion, MIN_GLIBC_VERSION);
  if (comparison === null) {
    return true;
  }

  return comparison >= 0;
}

function getInstalledSourcePackageRoot() {
  const packageJsonPath = requireFromHere.resolve('@cangjie-mcp/cangjie-mcp-source/package.json');
  return path.dirname(packageJsonPath);
}

function verifyTool(name) {
  const command = isWindows() ? 'where' : 'which';
  const result = spawnSync(command, [name], {
    stdio: 'ignore',
  });

  return result.status === 0;
}

function buildMissingToolsMessage() {
  const missing = ['cargo', 'rustc', 'protoc'].filter((tool) => !verifyTool(tool));
  if (missing.length === 0) {
    return null;
  }

  return `Missing build prerequisites: ${missing.join(', ')}.`;
}

function getSourceBuildOutputPath(platformInfo) {
  return path.join(
    getPackageRoot(),
    'artifacts',
    `${platformInfo.platform}-${platformInfo.arch}`,
    getBinaryFilename(platformInfo.platform)
  );
}

function buildFromSource(platformInfo) {
  const missingToolsMessage = buildMissingToolsMessage();
  if (missingToolsMessage) {
    throw new Error(
      `${missingToolsMessage} Source fallback is required for ${formatPlatformInfo(platformInfo)}.`
    );
  }

  const sourcePackageRoot = getInstalledSourcePackageRoot();
  const buildScript = path.join(sourcePackageRoot, 'lib', 'build.cjs');
  const outputPath = getSourceBuildOutputPath(platformInfo);
  const result = spawnSync(process.execPath, [buildScript, '--out', outputPath], {
    stdio: 'inherit',
  });

  if (result.status !== 0) {
    throw new Error(
      `Failed to build cangjie from source for ${formatPlatformInfo(platformInfo)}.`
    );
  }

  return outputPath;
}

function resolvePrebuiltBinary(platformInfo) {
  const packageName = getPackageName(platformInfo);
  if (!packageName) {
    return null;
  }

  try {
    const binaryPath = resolveBinaryFromPackageName(packageName, platformInfo.platform);
    if (fs.existsSync(binaryPath)) {
      return {
        packageName,
        binaryPath,
      };
    }
  } catch (error) {
    return null;
  }

  return null;
}

function createManifestFromBinary(platformInfo, strategy, binaryPath, packageName = null) {
  const packageRoot = getPackageRoot();

  return {
    version: MANIFEST_VERSION,
    strategy,
    packageName,
    binaryPath: path.relative(packageRoot, binaryPath),
    platform: platformInfo,
  };
}

async function installPackage() {
  const platformInfo = getPlatformInfo();
  const prebuilt = resolvePrebuiltBinary(platformInfo);
  const forceSourceBuild = shouldForceSourceBuild();
  const shouldUsePrebuilt =
    !forceSourceBuild && prebuilt && isModernLinuxSatisfied(platformInfo);

  if (shouldUsePrebuilt) {
    writeManifest(
      createManifestFromBinary(platformInfo, 'prebuilt', prebuilt.binaryPath, prebuilt.packageName)
    );
    console.log(`Using prebuilt binary for ${formatPlatformInfo(platformInfo)}.`);
    if (platformInfo.platform === 'linux' && !platformInfo.glibcVersion) {
      console.warn(
        'Unable to detect the glibc runtime version. If the binary fails to start, reinstall with CANGJIE_MCP_FORCE_BUILD=1.'
      );
    }
    return;
  }

  if (!prebuilt && platformInfo.platform !== 'linux' && !forceSourceBuild) {
    console.warn(
      `No prebuilt npm binary for ${formatPlatformInfo(platformInfo)}. Falling back to a local source build.`
    );
  } else if (platformInfo.platform === 'linux' && !isModernLinuxSatisfied(platformInfo)) {
    console.warn(
      `Detected ${formatPlatformInfo(platformInfo)} below glibc ${MIN_GLIBC_VERSION}. Falling back to a local source build.`
    );
  } else if (forceSourceBuild) {
    console.warn('CANGJIE_MCP_FORCE_BUILD=1 detected, building from source.');
  }

  const sourceRoot = getInstalledSourcePackageRoot();
  if (!fs.existsSync(path.join(sourceRoot, 'workspace', 'Cargo.toml'))) {
    console.warn(`Source workspace is not staged in ${sourceRoot}; skipping postinstall build in development checkout.`);
    return;
  }

  const builtBinaryPath = buildFromSource(platformInfo);
  writeManifest(createManifestFromBinary(platformInfo, 'source', builtBinaryPath));
}

function resolveBinaryPath() {
  const manifest = readManifest();
  const packageRoot = getPackageRoot();

  if (manifest) {
    const manifestBinaryPath = resolveBinaryFromManifest(packageRoot, manifest);
    if (fs.existsSync(manifestBinaryPath)) {
      return manifestBinaryPath;
    }

    if (manifest.packageName) {
      try {
        const prebuiltPath = resolveBinaryFromPackageName(manifest.packageName, process.platform);
        if (fs.existsSync(prebuiltPath)) {
          return prebuiltPath;
        }
      } catch (error) {
        // Fall through to a fresh resolution attempt below.
      }
    }
  }

  const platformInfo = getPlatformInfo();
  const prebuilt = resolvePrebuiltBinary(platformInfo);
  if (prebuilt) {
    return prebuilt.binaryPath;
  }

  throw new Error(
    `Unable to locate a cangjie binary for ${formatPlatformInfo(platformInfo)}. Reinstall the package or set CANGJIE_MCP_FORCE_BUILD=1.`
  );
}

function executeCli(args, options = {}) {
  return new Promise((resolve, reject) => {
    const binaryPath = resolveBinaryPath();
    const spawnArgs = [...(options.leadingArgs || []), ...args];
    const child = spawn(binaryPath, spawnArgs, {
      stdio: 'inherit',
      env: process.env,
    });

    child.on('error', (error) => {
      reject(new Error(`Failed to start ${options.commandName || 'cangjie'}: ${error.message}`));
    });

    child.on('exit', (code, signal) => {
      if (signal) {
        process.kill(process.pid, signal);
        return;
      }

      process.exitCode = code ?? 1;
      resolve();
    });
  });
}

module.exports = {
  compareVersions,
  createManifestFromBinary,
  detectGlibcVersion,
  executeCli,
  getBinaryFilename,
  getManifestPath,
  getPackageName,
  getPackageRoot,
  getPlatformInfo,
  installPackage,
  isModernLinuxSatisfied,
  readManifest,
  resolveBinaryFromManifest,
  resolveBinaryFromPackageName,
};
