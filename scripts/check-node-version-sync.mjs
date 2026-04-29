import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const workspaceRoot = fileURLToPath(new URL('../', import.meta.url));

const entries = [
  {
    label: 'factstr-node/package.json',
    path: join(workspaceRoot, 'factstr-node', 'package.json'),
    version: readPackageJsonVersion(join(workspaceRoot, 'factstr-node', 'package.json')),
  },
  {
    label: 'factstr-node/Cargo.toml',
    path: join(workspaceRoot, 'factstr-node', 'Cargo.toml'),
    version: readCargoTomlVersion(join(workspaceRoot, 'factstr-node', 'Cargo.toml')),
  },
  {
    label: 'factstr-node/npm/darwin-arm64/package.json',
    path: join(workspaceRoot, 'factstr-node', 'npm', 'darwin-arm64', 'package.json'),
    version: readPackageJsonVersion(
      join(workspaceRoot, 'factstr-node', 'npm', 'darwin-arm64', 'package.json'),
    ),
  },
  {
    label: 'factstr-node/npm/darwin-x64/package.json',
    path: join(workspaceRoot, 'factstr-node', 'npm', 'darwin-x64', 'package.json'),
    version: readPackageJsonVersion(
      join(workspaceRoot, 'factstr-node', 'npm', 'darwin-x64', 'package.json'),
    ),
  },
  {
    label: 'factstr-node/npm/linux-x64-gnu/package.json',
    path: join(workspaceRoot, 'factstr-node', 'npm', 'linux-x64-gnu', 'package.json'),
    version: readPackageJsonVersion(
      join(workspaceRoot, 'factstr-node', 'npm', 'linux-x64-gnu', 'package.json'),
    ),
  },
  {
    label: 'factstr-node/npm/win32-x64-msvc/package.json',
    path: join(workspaceRoot, 'factstr-node', 'npm', 'win32-x64-msvc', 'package.json'),
    version: readPackageJsonVersion(
      join(workspaceRoot, 'factstr-node', 'npm', 'win32-x64-msvc', 'package.json'),
    ),
  },
];

const expectedVersion = entries[0].version;
const mismatches = entries.filter((entry) => entry.version !== expectedVersion);

if (mismatches.length > 0) {
  console.error('factstr-node version drift detected:');
  for (const entry of entries) {
    console.error(`- ${entry.label}: ${entry.version}`);
  }
  process.exit(1);
}

console.log(`factstr-node version sync ok: ${expectedVersion}`);

function readPackageJsonVersion(path) {
  return JSON.parse(readFileSync(path, 'utf8')).version;
}

function readCargoTomlVersion(path) {
  const contents = readFileSync(path, 'utf8');
  const packageSection = contents.match(/\[package\][\s\S]*?(?=\n\[|$)/);
  if (!packageSection) {
    throw new Error(`Missing [package] section in ${path}`);
  }

  const version = packageSection[0].match(/^version\s*=\s*"([^"]+)"$/m);
  if (!version) {
    throw new Error(`Missing package version in ${path}`);
  }

  return version[1];
}
