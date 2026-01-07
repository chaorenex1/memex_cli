#!/usr/bin/env node

const { execFileSync, spawn } = require("child_process");
const { join } = require("path");
const { existsSync } = require("fs");

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

function getMemexEnvScriptPath() {
  const scriptExt = process.platform === "win32" ? ".ps1" : ".sh";
  const scriptName = `memex-env${scriptExt}`;

  try {
    // Try to find script in main package
    const mainPkgPath = require.resolve("memex-cli/package.json");
    const scriptPath = join(mainPkgPath, "..", "scripts", scriptName);

    if (existsSync(scriptPath)) {
      return scriptPath;
    }

    // Fallback: try platform-specific package
    const pkg = getPlatformPackage();
    const pkgPath = require.resolve(`${pkg}/package.json`);
    const fallbackPath = join(pkgPath, "..", "scripts", scriptName);

    if (existsSync(fallbackPath)) {
      return fallbackPath;
    }

    return null;
  } catch (e) {
    return null;
  }
}

function runMemexEnv(args) {
  const scriptPath = getMemexEnvScriptPath();

  if (!scriptPath) {
    console.error("⚠️  memex-env scripts not found in this package version");
    console.error("💡 Reinstall with: npm install -g memex-cli@latest");
    console.error("Or download manually from: https://github.com/chaorenex1/memex-cli");
    process.exit(1);
  }

  const platform = process.platform;
  let shellCmd, shellArgs;

  if (platform === "win32") {
    shellCmd = "powershell.exe";
    shellArgs = ["-ExecutionPolicy", "Bypass", "-File", scriptPath, ...args];
  } else {
    shellCmd = "bash";
    shellArgs = [scriptPath, ...args];
  }

  const child = spawn(shellCmd, shellArgs, {
    stdio: "inherit",
    shell: true
  });

  child.on("exit", code => {
    process.exit(code || 0);
  });

  child.on("error", err => {
    console.error(`Failed to run memex-env: ${err.message}`);
    process.exit(1);
  });
}

function main() {
  const args = process.argv.slice(2);

  // Check if memex-env command is requested
  if (args[0] === "env" || args[0] === "memex-env") {
    runMemexEnv(args.slice(1));
    return;
  }

  const binaryPath = getBinaryPath();

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
