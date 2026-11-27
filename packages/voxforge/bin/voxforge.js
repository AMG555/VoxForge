#!/usr/bin/env node

/**
 * VoxForge Standalone Native Runner
 * Automatically resolves, downloads, SHA-256 verifies, auto-refreshes,
 * and executes the platform-specific VoxForge binary with stdio isolation
 * for seamless Model Context Protocol (`npx github:AMG555/VoxForge mcp`) use.
 */

const fs = require('fs');
const path = require('path');
const os = require('os');
const https = require('https');
const crypto = require('crypto');
const { spawn } = require('child_process');

const REPO = 'AMG555/VoxForge';
const EXPLICIT_VERSION = process.env.VOXFORGE_VERSION || '';
let PKG_VERSION = '0.7.3';
try {
  PKG_VERSION = require('../package.json').version || PKG_VERSION;
} catch {
  // Fallback when invoked detached
}

function getTargetInfo() {
  const platform = process.platform;
  const arch = process.arch;

  let osName = '';
  let archName = '';
  let ext = '';

  switch (platform) {
    case 'win32':
      osName = 'windows';
      ext = '.exe';
      break;
    case 'darwin':
      osName = 'macos';
      break;
    case 'linux':
      osName = 'linux';
      break;
    default:
      console.error(`[voxforge] Unsupported platform: ${platform}`);
      process.exit(1);
  }

  switch (arch) {
    case 'x64':
      archName = 'x86_64';
      break;
    case 'arm64':
      archName = 'aarch64';
      break;
    default:
      console.error(`[voxforge] Unsupported architecture: ${arch}`);
      process.exit(1);
  }

  const binaryName = `voxforge-${osName}-${archName}${ext}`;
  return { binaryName, osName, archName, ext };
}

function fetchText(url) {
  return new Promise((resolve, reject) => {
    function makeRequest(currentUrl, hops = 0) {
      if (hops > 5) return reject(new Error('Too many redirects'));
      https
        .get(currentUrl, { headers: { 'User-Agent': 'VoxForge-Runner' } }, (response) => {
          if (response.statusCode >= 300 && response.statusCode < 400 && response.headers.location) {
            makeRequest(response.headers.location, hops + 1);
            return;
          }
          if (response.statusCode !== 200) {
            reject(new Error(`HTTP ${response.statusCode}`));
            return;
          }
          let body = '';
          response.setEncoding('utf8');
          response.on('data', (chunk) => {
            body += chunk;
          });
          response.on('end', () => resolve(body));
        })
        .on('error', reject);
    }
    makeRequest(url);
  });
}

function downloadBinary(url, destPath, expectedSha256) {
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(destPath);
    const hasher = crypto.createHash('sha256');
    // Always log diagnostics to stderr so `voxforge mcp` stdio remains pure JSON-RPC 2.0
    console.error(`[voxforge] Downloading native workstation runtime from ${url}...`);

    function makeRequest(currentUrl, hops = 0) {
      if (hops > 5) {
        file.close();
        fs.unlink(destPath, () => {});
        return reject(new Error('Too many redirects'));
      }
      https
        .get(currentUrl, { headers: { 'User-Agent': 'VoxForge-Runner' } }, (response) => {
          if (response.statusCode >= 300 && response.statusCode < 400 && response.headers.location) {
            makeRequest(response.headers.location, hops + 1);
            return;
          }

          if (response.statusCode !== 200) {
            file.close();
            fs.unlink(destPath, () => {});
            reject(new Error(`Server returned HTTP ${response.statusCode}`));
            return;
          }

          let downloadedBytes = 0;
          const totalBytes = parseInt(response.headers['content-length'] || '0', 10);

          response.on('data', (chunk) => {
            downloadedBytes += chunk.length;
            hasher.update(chunk);
            if (totalBytes > 0 && process.stderr.isTTY) {
              const pct = Math.round((downloadedBytes / totalBytes) * 100);
              process.stderr.write(
                `\r[voxforge] Downloading: ${pct}% (${(downloadedBytes / 1048576).toFixed(1)} MB)`
              );
            }
          });

          response.pipe(file);

          file.on('finish', () => {
            file.close();
            if (process.stderr.isTTY) process.stderr.write('\n');
            const actualSha = hasher.digest('hex').toLowerCase();
            if (expectedSha256 && actualSha !== expectedSha256.toLowerCase()) {
              fs.unlink(destPath, () => {});
              reject(new Error(`SHA-256 verification failed: expected ${expectedSha256}, got ${actualSha}`));
              return;
            }
            console.error(
              expectedSha256
                ? `[voxforge] Download & SHA-256 verification succeeded.`
                : `[voxforge] Download completed successfully.`
            );
            resolve();
          });
        })
        .on('error', (err) => {
          file.close();
          fs.unlink(destPath, () => {});
          reject(err);
        });
    }

    makeRequest(url);
  });
}

function parseChecksums(text) {
  const map = new Map();
  for (const line of String(text || '').split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    const m = trimmed.match(/^([0-9a-fA-F]{64})\s+[*]?\.?\/?(.+)$/);
    if (m) {
      map.set(m[2].trim(), m[1].toLowerCase());
    }
  }
  return map;
}

async function main() {
  const { binaryName, ext } = getTargetInfo();
  const cacheDir = path.join(os.homedir(), '.voxforge', 'bin');
  const binaryPath = path.join(cacheDir, `voxforge${ext}`);
  const versionStampPath = path.join(cacheDir, '.version');
  const desiredTag = EXPLICIT_VERSION || `v${PKG_VERSION}`;

  const localCandidates = [
    path.resolve(__dirname, '../../../target/release-small', `voxforge${ext}`),
    path.resolve(__dirname, '../../../target/release', `voxforge${ext}`),
    path.resolve(__dirname, '../../../target/debug', `voxforge${ext}`),
    path.resolve(process.cwd(), 'target/release-small', `voxforge${ext}`),
    path.resolve(process.cwd(), 'target/release', `voxforge${ext}`),
    path.resolve(process.cwd(), 'target/debug', `voxforge${ext}`),
  ];

  for (const candidate of localCandidates) {
    if (fs.existsSync(candidate)) {
      runBinary(candidate);
      return;
    }
  }

  let cachedTag = '';
  try {
    if (fs.existsSync(versionStampPath)) {
      cachedTag = fs.readFileSync(versionStampPath, 'utf8').trim();
    }
  } catch {
    cachedTag = '';
  }

  const needsDownload = !fs.existsSync(binaryPath) || cachedTag !== desiredTag;

  if (needsDownload) {
    fs.mkdirSync(cacheDir, { recursive: true });

    const candidateNames = [];
    if (process.platform === 'darwin') {
      candidateNames.push('voxforge-macos-universal', binaryName, 'voxforge-macos-arm64', 'voxforge-macos-x86_64');
    } else if (process.platform === 'linux') {
      candidateNames.push(binaryName);
    } else {
      candidateNames.push(binaryName, 'voxforge-windows-x86_64.exe', 'voxforge.exe');
    }

    const baseReleaseUrl = EXPLICIT_VERSION
      ? `https://github.com/${REPO}/releases/download/${EXPLICIT_VERSION}`
      : `https://github.com/${REPO}/releases/latest/download`;

    let checksums = new Map();
    try {
      const sumsText = await fetchText(`${baseReleaseUrl}/checksums.txt`);
      checksums = parseChecksums(sumsText);
    } catch {
      // Optional if release has not finished uploading checksums
    }

    const tmpBinaryPath = `${binaryPath}.tmp`;
    let downloaded = false;
    for (const name of candidateNames) {
      const downloadUrl = `${baseReleaseUrl}/${name}`;
      try {
        await downloadBinary(downloadUrl, tmpBinaryPath, checksums.get(name));
        if (process.platform !== 'win32') {
          fs.chmodSync(tmpBinaryPath, 0o755);
        }
        if (fs.existsSync(binaryPath)) {
          fs.unlinkSync(binaryPath);
        }
        fs.renameSync(tmpBinaryPath, binaryPath);
        if (process.platform === 'darwin') {
          try {
            const { execSync } = require('child_process');
            execSync(`xattr -d com.apple.quarantine "${binaryPath}"`, { stdio: 'ignore' });
          } catch {
            // Non-fatal if attribute not present
          }
        }
        fs.writeFileSync(versionStampPath, desiredTag, 'utf8');
        downloaded = true;
        break;
      } catch (err) {
        if (fs.existsSync(tmpBinaryPath)) {
          fs.unlinkSync(tmpBinaryPath);
        }
      }
    }

    if (!downloaded && !fs.existsSync(binaryPath)) {
      console.error('[voxforge] Pre-built binary download not available from GitHub releases.');
      console.error('[voxforge] Checking for local cargo installation...');

      try {
        const check = spawn(process.platform === 'win32' ? 'where' : 'which', ['voxforge']);
        check.on('close', (code) => {
          if (code === 0) {
            runBinary('voxforge');
          } else {
            console.error('[voxforge] Please run: cargo install --git https://github.com/AMG555/VoxForge.git voxforge-cli');
            process.exit(1);
          }
        });
        return;
      } catch {
        process.exit(1);
      }
    }
  }

  runBinary(binaryPath);
}

function runBinary(executable) {
  let args = process.argv.slice(2);
  if (args.length === 0) {
    args = ['serve', '--open'];
  }

  const child = spawn(executable, args, { stdio: 'inherit' });

  child.on('exit', (code, signal) => {
    if (signal) {
      process.kill(process.pid, signal);
    } else {
      process.exit(code || 0);
    }
  });

  child.on('error', (err) => {
    console.error(`[voxforge] Process execution error: ${err.message}`);
    process.exit(1);
  });
}

main().catch((err) => {
  console.error(`[voxforge] Fatal error: ${err.message}`);
  process.exit(1);
});
