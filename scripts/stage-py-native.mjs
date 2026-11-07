import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, '..');

const platform = process.platform;
let binaryName;
let targetExt;

if (platform === 'darwin') {
  binaryName = 'liblavi_py.dylib';
  targetExt = '.abi3.so';
} else if (platform === 'win32') {
  binaryName = 'lavi_py.dll';
  targetExt = '.pyd';
} else {
  binaryName = 'liblavi_py.so';
  targetExt = '.abi3.so';
}

const targetDir = path.join(rootDir, 'target');
const profile = process.env.CARGO_PROFILE || 'debug';
const srcFile = path.join(targetDir, profile, binaryName);

if (!fs.existsSync(srcFile)) {
  console.error(`Error: Native binary not found at ${srcFile}`);
  console.error(`Please run cargo build -p lavi-py first.`);
  process.exit(1);
}

const destDir = path.join(rootDir, 'lavi-py', 'python', 'lavi');
fs.mkdirSync(destDir, { recursive: true });

const destFile = path.join(destDir, `_lavi_py${targetExt}`);
fs.copyFileSync(srcFile, destFile);
console.log(`Staged native binary: ${srcFile} -> ${destFile}`);
