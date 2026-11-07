#!/usr/bin/env node
import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const DEFAULT_ROOT = path.resolve(__dirname, '..');

export function createNpmPackageManifest(version) {
  return {
    name: 'lavi-fsm',
    version,
    description: 'Portable XState v5 statechart runtime powered by Rust and WebAssembly.',
    license: 'MIT',
    repository: {
      type: 'git',
      url: 'git+https://github.com/emmby/lavi.git',
      directory: 'lavi-wasm',
    },
    homepage: 'https://emmby.github.io/lavi/examples/',
    keywords: ['xstate', 'statechart', 'state-machine', 'fsm', 'wasm', 'lavi'],
    files: ['node/', 'bundler/', 'web/', 'README.md', 'LICENSE'],
    main: './node/lavi.js',
    module: './bundler/lavi.js',
    types: './node/lavi.d.ts',
    exports: {
      '.': {
        types: './node/lavi.d.ts',
        node: './node/lavi.js',
        browser: './bundler/lavi.js',
        import: './bundler/lavi.js',
        default: './node/lavi.js',
      },
      './web': {
        types: './web/lavi.d.ts',
        import: './web/lavi.js',
        default: './web/lavi.js',
      },
      './package.json': './package.json',
    },
  };
}

export function assembleNpmPackageDir(options = {}) {
  const rootDir = options.rootDir || DEFAULT_ROOT;
  const outDir = options.outDir || path.join(rootDir, 'dist', 'npm');
  const skipWasmBuild = options.skipWasmBuild ?? false;

  const rootPkg = JSON.parse(fs.readFileSync(path.join(rootDir, 'package.json'), 'utf8'));
  const version = options.version || rootPkg.version || '0.1.0';

  fs.mkdirSync(outDir, { recursive: true });

  const targets = [
    { wasmTarget: 'nodejs', subDir: 'node' },
    { wasmTarget: 'bundler', subDir: 'bundler' },
    { wasmTarget: 'web', subDir: 'web' },
  ];

  if (!skipWasmBuild) {
    const wasmCrateDir = path.join(rootDir, 'lavi-wasm');
    for (const t of targets) {
      const targetOutDir = path.join(outDir, t.subDir);
      const res = spawnSync(
        'wasm-pack',
        ['build', '--release', '--target', t.wasmTarget, '--out-dir', targetOutDir, '--out-name', 'lavi'],
        {
          cwd: wasmCrateDir,
          stdio: 'inherit',
        }
      );
      if (res.status !== 0) {
        throw new Error(`wasm-pack build failed for target ${t.wasmTarget} with exit code ${res.status}`);
      }
    }
  }

  for (const t of targets) {
    const subPath = path.join(outDir, t.subDir);
    if (!fs.existsSync(subPath)) continue;
    for (const unwanted of ['.gitignore', 'package.json', 'README.md', 'LICENSE']) {
      const candidate = path.join(subPath, unwanted);
      if (fs.existsSync(candidate)) {
        fs.unlinkSync(candidate);
      }
    }
  }

  const readmeSrc = path.join(rootDir, 'lavi-wasm', 'README.md');
  if (fs.existsSync(readmeSrc)) {
    fs.copyFileSync(readmeSrc, path.join(outDir, 'README.md'));
  }

  const licenseSrc = path.join(rootDir, 'LICENSE');
  if (fs.existsSync(licenseSrc)) {
    fs.copyFileSync(licenseSrc, path.join(outDir, 'LICENSE'));
  }

  const manifest = createNpmPackageManifest(version);
  const manifestPath = path.join(outDir, 'package.json');
  fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + '\n', 'utf8');

  return {
    outDir,
    manifestPath,
    manifest,
  };
}

const isMain =
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url));

if (isMain) {
  const skipWasmBuild = process.argv.includes('--skip-wasm-build');
  const res = assembleNpmPackageDir({ skipWasmBuild });
  console.log(`✅ Assembled unified lavi-fsm@${res.manifest.version} npm package at ${res.outDir}`);
}
