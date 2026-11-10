import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, '..');

const platform = process.platform;
let binaryName;

if (platform === 'darwin') {
  binaryName = 'liblavi_c_cpp.dylib';
} else if (platform === 'win32') {
  binaryName = 'lavi_c_cpp.dll';
} else {
  binaryName = 'liblavi_c_cpp.so';
}

const targetDir = path.join(rootDir, 'target');
const profile = process.env.CARGO_PROFILE || 'debug';
const srcFile = path.join(targetDir, profile, binaryName);
const destDir = path.join(rootDir, 'lavi-csharp', 'native');
const destFile = path.join(destDir, binaryName);

fs.mkdirSync(destDir, { recursive: true });

if (fs.existsSync(srcFile)) {
  fs.copyFileSync(srcFile, destFile);
  // Remove shared library from target/<profile> so C/C++/Swift linker tests always link liblavi_c_cpp.a statically
  fs.unlinkSync(srcFile);
  console.log(`Staged C# P/Invoke native binary: ${srcFile} -> ${destFile}`);
} else if (fs.existsSync(destFile)) {
  console.log(`C# P/Invoke native binary already staged at: ${destFile}`);
} else {
  console.error(`Error: Native binary not found at ${srcFile}`);
  console.error(`Please run: cargo rustc --package lavi-c-cpp --lib --crate-type cdylib`);
  process.exit(1);
}
