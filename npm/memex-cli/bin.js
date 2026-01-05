#!/usr/bin/env node

const { execFileSync } = require("child_process");
const { join } = require("path");

const PLATFORMS = {
  "darwin-arm64": "@memex-cli/darwin-arm64",
  "darwin-x64": "@memex-cli/darwin-x64",
  "linux-x64": "@memex-cli/linux-x64",
  "win32-x64": "@memex-cli/win32-x64",
};

function getPlatformPackage() {
  const platform = process.platform;
  const arch = process.arch;
  const key = `${platform}-${arch}`;

  const pkg = PLATFORMS[key];
  if (!pkg) {
    console.error(`Unsupported platform: ${key}`);
    console.error(`Supported platforms: ${Object.keys(PLATFORMS).join(", ")}`);
    process.exit(1);
  }
  return pkg;
}

function getBinaryPath() {
  const pkg = getPlatformPackage();
  const binaryName = process.platform === "win32" ? "memex-cli.exe" : "memex-cli";

  try {
    const pkgPath = require.resolve(`${pkg}/package.json`);
    return join(pkgPath, "..", binaryName);
  } catch (e) {
    console.error(`Failed to find package ${pkg}`);
    console.error("Please ensure the package is installed correctly.");
    console.error("Try running: npm install -g memex-cli");
    process.exit(1);
  }
}

function main() {
  const binaryPath = getBinaryPath();
  const args = process.argv.slice(2);

  try {
    execFileSync(binaryPath, args, {
      stdio: "inherit",
      env: process.env,
    });
  } catch (e) {
    if (e.status !== undefined) {
      process.exit(e.status);
    }
    throw e;
  }
}

main();
