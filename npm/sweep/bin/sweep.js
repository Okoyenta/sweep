#!/usr/bin/env node
"use strict";

// Thin launcher for the platform-specific sweep binary. The binary itself is
// shipped in an optional dependency, so npm installs only the one matching the
// host — the same layout esbuild and biome use.

const { spawnSync } = require("node:child_process");
const path = require("node:path");

const PLATFORM_PACKAGES = {
  "win32-x64": {
    pkg: "@okoyenta/sweep-win32-x64",
    binary: "sweep.exe",
  },
  "linux-x64": {
    pkg: "@okoyenta/sweep-linux-x64",
    binary: "sweep",
  },
};

const target = `${process.platform}-${process.arch}`;
const platform = PLATFORM_PACKAGES[target];

if (!platform) {
  process.stderr.write(
    `sweep: no prebuilt binary for ${target}.\n` +
      `Supported platforms: ${Object.keys(PLATFORM_PACKAGES).join(", ")}.\n` +
      `Build from source instead: https://github.com/Okoyenta/sweep\n`
  );
  process.exit(1);
}

let binary;
try {
  const pkgDir = path.dirname(require.resolve(`${platform.pkg}/package.json`));
  binary = path.join(pkgDir, platform.binary);
} catch {
  process.stderr.write(
    `sweep: the optional dependency ${platform.pkg} is not installed.\n` +
      `This happens when npm runs with --no-optional or --omit=optional.\n` +
      `Reinstall with: npm install -g @okoyenta/sweep\n`
  );
  process.exit(1);
}

// stdio: "inherit" is what keeps `sweep tui` interactive — the child needs the
// real terminal, not a pipe.
const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });

if (result.error) {
  process.stderr.write(`sweep: failed to run ${binary}: ${result.error.message}\n`);
  process.exit(1);
}

// status is null when the child died from a signal; there is no exit code to
// forward in that case.
process.exit(result.status === null ? 1 : result.status);
