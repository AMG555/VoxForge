#!/usr/bin/env node

/**
 * VoxForge Standalone Native Runner
 * Automatically resolves, downloads, and executes the platform-specific
 * VoxForge binary with embedded workstation UI and neural runtime.
 */

const fs = require('fs');
const path = require('path');
const os = require('os');
const https = require('https');
const { spawn } = require('child_process');

const REPO = 'AMG555/VoxForge';
const pkg = require('../package.json');
const DEFAULT_VERSION = process.env.VOXFORGE_VERSION || `v${pkg.version}`;

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

function downloadBinary(url, destPath) {
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(destPath);
    console.log(`[voxforge] Downloading native workstation runtime from ${url}...`);

    function makeRequest(currentUrl) {
      https.get(currentUrl, { headers: { 'User-Agent': 'VoxForge-Runner' } }, (response) => {
        if (response.statusCode >= 300 && response.statusCode < 400 && response.headers.location) {
          makeRequest(response.headers.location);
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
          if (totalBytes > 0 && process.stdout.isTTY) {
            const pct = Math.round((downloadedBytes / totalBytes) * 100);
            process.stdout.write(`\r[voxforge] Downloading: ${pct}% (${(downloadedBytes / 1048576).toFixed(1)} MB)`);
          }
        });

        response.pipe(file);

        file.on('finish', () => {
          file.close();
          if (process.stdout.isTTY) process.stdout.write('\n');
          console.log('[voxforge] Download completed successfully.');
          resolve();
        });
      }).on('error', (err) => {
        file.close();
        fs.unlink(destPath, () => {});
        reject(err);
      });
    }

    makeRequest(url);
  });
}

async function main() {
  const { binaryName, ext } = getTargetInfo();
  const cacheDir = path.join(os.homedir(), '.voxforge', 'bin');
  const binaryPath = path.join(cacheDir, `voxforge${ext}`);

  // 1. Check if running inside or near repository with local build (release or debug)
  const localCandidates = [
    path.resolve(__dirname, '../../../target/release', `voxforge${ext}`),
    path.resolve(__dirname, '../../../target/debug', `voxforge${ext}`),
    path.resolve(process.cwd(), 'target/release', `voxforge${ext}`),
    path.resolve(process.cwd(), 'target/debug', `voxforge${ext}`),
  ];
  for (const candidate of localCandidates) {
    if (fs.existsSync(candidate)) {
      runBinary(candidate);
      return;
    }
  }

  if (!fs.existsSync(binaryPath)) {
    fs.mkdirSync(cacheDir, { recursive: true });

    const candidateNames = [];
    if (process.platform === 'darwin') {
      candidateNames.push('voxforge-macos-universal', binaryName, 'voxforge-macos-arm64', 'voxforge-macos-x86_64');
    } else if (process.platform === 'linux') {
      candidateNames.push(binaryName, 'voxforge-linux-x86_64');
    } else {
      candidateNames.push(binaryName);
    }

    let downloaded = false;
    for (const name of candidateNames) {
      const downloadUrl = `https://github.com/${REPO}/releases/download/${DEFAULT_VERSION}/${name}`;
      try {
        await downloadBinary(downloadUrl, binaryPath);
        if (process.platform !== 'win32') {
          fs.chmodSync(binaryPath, 0o755);
        }
        if (process.platform === 'darwin') {
          try {
            const { execSync } = require('child_process');
            execSync(`xattr -d com.apple.quarantine "${binaryPath}"`, { stdio: 'ignore' });
          } catch {
            // Non-fatal if attribute not present
          }
        }
        downloaded = true;
        break;
      } catch (err) {
        // Continue to next candidate
      }
    }

    if (!downloaded) {
      console.warn('[voxforge] Pre-built binary download not available from GitHub releases.');
      console.log('[voxforge] Checking for local cargo installation...');

      // Fallback: Check if user has voxforge in PATH or cargo
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
  // Default to serve --open if no arguments specified
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
