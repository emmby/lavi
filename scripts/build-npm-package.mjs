#!/usr/bin/env node
import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const DEFAULT_ROOT = path.resolve(__dirname, '..');

export const REACT_NATIVE_RUSTFLAGS =
  '-C target-feature=-reference-types,-multivalue,-nontrapping-fptoint -C link-arg=--no-check-features -C link-arg=--features=mutable-globals,sign-ext,bulk-memory';

export const UTF8_POLYFILL_SNIPPET = `function __laviUtf8DecodeFallback(bytes) {
    let i = 0;
    const len = bytes.length;
    if (len >= 3 && bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf) {
        i = 3;
    }
    let out = '';
    while (i < len) {
        const b0 = bytes[i++];
        if (b0 < 0x80) {
            out += String.fromCharCode(b0);
        } else if ((b0 & 0xe0) === 0xc0 && i < len) {
            const b1 = bytes[i++] & 0x3f;
            out += String.fromCharCode(((b0 & 0x1f) << 6) | b1);
        } else if ((b0 & 0xf0) === 0xe0 && i + 1 < len) {
            const b1 = bytes[i++] & 0x3f;
            const b2 = bytes[i++] & 0x3f;
            out += String.fromCharCode(((b0 & 0x0f) << 12) | (b1 << 6) | b2);
        } else if ((b0 & 0xf8) === 0xf0 && i + 2 < len) {
            const b1 = bytes[i++] & 0x3f;
            const b2 = bytes[i++] & 0x3f;
            const b3 = bytes[i++] & 0x3f;
            const cp = ((b0 & 0x07) << 18) | (b1 << 12) | (b2 << 6) | b3;
            out += String.fromCodePoint(cp);
        } else {
            out += '\\uFFFD';
        }
    }
    return out;
}

function __laviUtf8EncodeFallback(str) {
    const out = [];
    for (let i = 0; i < str.length; i++) {
        const c = str.charCodeAt(i);
        if (c < 0x80) {
            out.push(c);
        } else if (c < 0x800) {
            out.push(0xc0 | (c >> 6), 0x80 | (c & 0x3f));
        } else if (c >= 0xd800 && c <= 0xdbff && i + 1 < str.length) {
            const c2 = str.charCodeAt(i + 1);
            if (c2 >= 0xdc00 && c2 <= 0xdfff) {
                const cp = ((c - 0xd800) << 10) + (c2 - 0xdc00) + 0x10000;
                out.push(
                    0xf0 | (cp >> 18),
                    0x80 | ((cp >> 12) & 0x3f),
                    0x80 | ((cp >> 6) & 0x3f),
                    0x80 | (cp & 0x3f)
                );
                i++;
            } else {
                out.push(0xef, 0xbf, 0xbd);
            }
        } else if (c >= 0xdc00 && c <= 0xdfff) {
            out.push(0xef, 0xbf, 0xbd);
        } else {
            out.push(
                0xe0 | (c >> 12),
                0x80 | ((c >> 6) & 0x3f),
                0x80 | (c & 0x3f)
            );
        }
    }
    return new Uint8Array(out);
}

function __laviCreateTextDecoder() {
    if (typeof globalThis !== 'undefined' && globalThis.__LAVI_FORCE_UTF8_FALLBACK__ === true) {
        return { decode: __laviUtf8DecodeFallback };
    }
    if (typeof TextDecoder !== 'undefined') {
        try {
            // Note: Do not pass { fatal: true } because React Native Hermes throws RangeError on fatal: true
            const dec = new TextDecoder('utf-8', { ignoreBOM: true });
            dec.decode(new Uint8Array(0));
            return dec;
        } catch (_) {
            try {
                const dec = new TextDecoder();
                dec.decode(new Uint8Array(0));
                return dec;
            } catch (_2) {}
        }
    }
    return { decode: __laviUtf8DecodeFallback };
}

function __laviCreateTextEncoder() {
    let enc = null;
    if (
        !(typeof globalThis !== 'undefined' && globalThis.__LAVI_FORCE_UTF8_FALLBACK__ === true) &&
        typeof TextEncoder !== 'undefined'
    ) {
        try {
            const candidate = new TextEncoder();
            candidate.encode('');
            enc = candidate;
        } catch (_) {}
    }
    if (!enc) {
        enc = { encode: __laviUtf8EncodeFallback };
    }
    if (typeof enc.encodeInto !== 'function') {
        enc.encodeInto = function (arg, view) {
            const buf = enc.encode(arg);
            view.set(buf);
            return {
                read: arg.length,
                written: buf.length
            };
        };
    }
    return enc;
}

let cachedTextDecoder = __laviCreateTextDecoder();
function decodeText(ptr, len) {
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = __laviCreateTextEncoder();`;

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
    keywords: [
      'xstate',
      'statechart',
      'state-machine',
      'fsm',
      'wasm',
      'lavi',
      'react',
      'react-native',
      'expo',
    ],
    files: ['node/', 'bundler/', 'web/', 'react-native/', 'react/', 'README.md', 'LICENSE'],
    main: './node/lavi.js',
    module: './bundler/lavi.js',
    'react-native': './react-native/lavi.js',
    types: './node/lavi.d.ts',
    peerDependencies: {
      react: '>=18.0.0',
    },
    peerDependenciesMeta: {
      react: {
        optional: true,
      },
    },
    exports: {
      '.': {
        types: './node/lavi.d.ts',
        'react-native': './react-native/lavi.js',
        node: './node/lavi.js',
        browser: './bundler/lavi.js',
        import: './bundler/lavi.js',
        default: './node/lavi.js',
      },
      './react-native': {
        types: './react-native/lavi.d.ts',
        import: './react-native/lavi.js',
        default: './react-native/lavi.js',
      },
      './react': {
        types: './react/index.d.ts',
        'react-native': './react/index.react-native.js',
        node: './react/index.cjs',
        browser: './react/index.js',
        import: './react/index.js',
        default: './react/index.cjs',
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

export function resolveWasm2JsCommand(rootDir = DEFAULT_ROOT) {
  const localBin = path.join(
    rootDir,
    'node_modules',
    '.bin',
    process.platform === 'win32' ? 'wasm2js.cmd' : 'wasm2js'
  );
  if (fs.existsSync(localBin)) {
    return { command: localBin, prefixArgs: [] };
  }
  const whichRes = spawnSync(process.platform === 'win32' ? 'where' : 'which', ['wasm2js'], {
    stdio: 'pipe',
  });
  if (whichRes.status === 0) {
    return { command: 'wasm2js', prefixArgs: [] };
  }
  return { command: 'npx', prefixArgs: ['--yes', '-p', 'binaryen', 'wasm2js'] };
}

export function patchReactNativeWasm2JsDir(rnDir) {
  const wasmJsPath = path.join(rnDir, 'lavi_bg.wasm.js');
  const laviJsPath = path.join(rnDir, 'lavi.js');

  if (!fs.existsSync(wasmJsPath)) {
    throw new Error(`Missing wasm2js output file: ${wasmJsPath}`);
  }
  if (!fs.existsSync(laviJsPath)) {
    throw new Error(`Missing wasm-bindgen JS wrapper file: ${laviJsPath}`);
  }

  let wasmJs = fs.readFileSync(wasmJsPath, 'utf8');
  const placeholderImportPattern =
    /^\s*import\s+\*\s+as\s+__wbindgen_placeholder__\s+from\s+['"]__wbindgen_placeholder__['"];?\s*/m;
  const retAsmFuncPattern =
    /var\s+retasmFunc\s*=\s*asmFunc\(\{\s*["']__wbindgen_placeholder__["']:\s*__wbindgen_placeholder__,?\s*\}\);[\s\S]*$/;
  if (!placeholderImportPattern.test(wasmJs)) {
    throw new Error(`Could not locate __wbindgen_placeholder__ import in ${wasmJsPath}`);
  }
  if (!retAsmFuncPattern.test(wasmJs)) {
    throw new Error(`Could not locate retasmFunc export block in ${wasmJsPath}`);
  }
  wasmJs = wasmJs.replace(placeholderImportPattern, '');
  wasmJs = wasmJs.replace(retAsmFuncPattern, 'module.exports = { asmFunc };\n');
  fs.writeFileSync(wasmJsPath, wasmJs, 'utf8');

  let laviJs = fs.readFileSync(laviJsPath, 'utf8');
  const textCodecPattern =
    /let cachedTextDecoder = new TextDecoder\([\s\S]*?const cachedTextEncoder = new TextEncoder\(\);\s*if \(!\('encodeInto' in cachedTextEncoder\)\) \{[\s\S]*?\n\}/m;
  if (!textCodecPattern.test(laviJs)) {
    throw new Error(`Could not locate TextDecoder/TextEncoder block in ${laviJsPath}`);
  }
  laviJs = laviJs.replace(textCodecPattern, UTF8_POLYFILL_SNIPPET);

  const newFunctionPattern = /new Function\(getStringFromWasm0\(arg0,\s*arg1\)\)/g;
  if (!newFunctionPattern.test(laviJs)) {
    throw new Error(`Could not locate new Function(getStringFromWasm0(arg0, arg1)) in ${laviJsPath}`);
  }
  laviJs = laviJs.replace(
    newFunctionPattern,
    `(function(body) { return body.indexOf('throw ') !== -1 ? function() { throw new Error("This isn't supposed to be called"); } : function() { return undefined; }; })(getStringFromWasm0(arg0, arg1))`
  );

  const wasmLoadPattern =
    /const wasmPath = `\$\{__dirname\}\/lavi_bg\.wasm`;[\s\S]*?const wasm = exports\.__wasm = new WebAssembly\.Instance\(wasmModule, imports\)\.exports;(?:\s*wasm\.__wbindgen_start\(\);)?/m;
  if (!wasmLoadPattern.test(laviJs)) {
    throw new Error(`Could not locate WebAssembly.Instance loader block in ${laviJsPath}`);
  }
  laviJs = laviJs.replace(
    wasmLoadPattern,
    `const { asmFunc } = require('./lavi_bg.wasm.js');\nconst wasm = exports.__wasm = asmFunc(imports);\nif (typeof wasm.__wbindgen_start === 'function') {\n    wasm.__wbindgen_start();\n}`
  );
  fs.writeFileSync(laviJsPath, laviJs, 'utf8');

  fs.writeFileSync(
    path.join(rnDir, 'index.js'),
    `'use strict';\nmodule.exports = require('./lavi.js');\n`,
    'utf8'
  );
  fs.writeFileSync(
    path.join(rnDir, 'index.d.ts'),
    `export * from './lavi.js';\n`,
    'utf8'
  );
  fs.writeFileSync(
    path.join(rnDir, 'package.json'),
    JSON.stringify(
      {
        name: 'lavi-fsm-react-native',
        private: true,
        main: './lavi.js',
        'react-native': './lavi.js',
        types: './lavi.d.ts',
      },
      null,
      2
    ) + '\n',
    'utf8'
  );

  for (const extra of ['lavi_bg.wasm', 'lavi_bg.wasm.d.ts']) {
    const p = path.join(rnDir, extra);
    if (fs.existsSync(p)) {
      fs.unlinkSync(p);
    }
  }
}

export function stageReactSubpackage(
  rootDir = DEFAULT_ROOT,
  targetPkgDir,
  options = {}
) {
  const reactSrcDir = path.join(rootDir, 'lavi-wasm', 'react');
  if (!fs.existsSync(reactSrcDir)) {
    return;
  }
  const reactOutDir = path.join(targetPkgDir, 'react');
  fs.mkdirSync(reactOutDir, { recursive: true });
  for (const entry of fs.readdirSync(reactSrcDir)) {
    const srcFile = path.join(reactSrcDir, entry);
    const dstFile = path.join(reactOutDir, entry);
    if (fs.statSync(srcFile).isFile()) {
      fs.copyFileSync(srcFile, dstFile);
    }
  }
  const dtsPath = path.join(reactOutDir, 'index.d.ts');
  const dctsPath = path.join(reactOutDir, 'index.d.cts');
  if (fs.existsSync(dtsPath)) {
    fs.copyFileSync(dtsPath, dctsPath);
  }
  if (options.flatParentDir) {
    for (const relFile of ['index.cjs', 'index.d.ts', 'index.d.cts']) {
      const filePath = path.join(reactOutDir, relFile);
      if (fs.existsSync(filePath)) {
        const content = fs
          .readFileSync(filePath, 'utf8')
          .replace(/\.\.\/node\/lavi\.js/g, '../lavi.js');
        fs.writeFileSync(filePath, content, 'utf8');
      }
    }
  }
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
    { wasmTarget: 'nodejs', subDir: 'react-native', isReactNativeWasm2Js: true },
  ];

  if (!skipWasmBuild) {
    const wasmCrateDir = path.join(rootDir, 'lavi-wasm');
    for (const t of targets) {
      const targetOutDir = path.join(outDir, t.subDir);
      const env = { ...process.env };
      if (t.isReactNativeWasm2Js) {
        env.RUSTFLAGS = env.RUSTFLAGS
          ? `${env.RUSTFLAGS} ${REACT_NATIVE_RUSTFLAGS}`
          : REACT_NATIVE_RUSTFLAGS;
      }
      const res = spawnSync(
        'wasm-pack',
        [
          'build',
          '--release',
          '--target',
          t.wasmTarget,
          '--out-dir',
          targetOutDir,
          '--out-name',
          'lavi',
        ],
        {
          cwd: wasmCrateDir,
          stdio: 'inherit',
          env,
        }
      );
      if (res.status !== 0) {
        throw new Error(
          `wasm-pack build failed for target ${t.subDir} (${t.wasmTarget}) with exit code ${res.status}`
        );
      }

      if (t.isReactNativeWasm2Js) {
        const { command, prefixArgs } = resolveWasm2JsCommand(rootDir);
        const wasmFile = path.join(targetOutDir, 'lavi_bg.wasm');
        const wasmJsFile = path.join(targetOutDir, 'lavi_bg.wasm.js');
        const w2jRes = spawnSync(
          command,
          [
            ...prefixArgs,
            '--enable-bulk-memory',
            '--enable-sign-ext',
            '--enable-mutable-globals',
            '-O3',
            wasmFile,
            '-o',
            wasmJsFile,
          ],
          {
            cwd: wasmCrateDir,
            stdio: 'inherit',
          }
        );
        if (w2jRes.status !== 0) {
          throw new Error(`wasm2js failed with exit code ${w2jRes.status}`);
        }
        patchReactNativeWasm2JsDir(targetOutDir);
      }
    }
  }

  for (const t of targets) {
    const subPath = path.join(outDir, t.subDir);
    if (!fs.existsSync(subPath)) continue;
    const unwantedList = t.isReactNativeWasm2Js
      ? ['.gitignore', 'README.md', 'LICENSE']
      : ['.gitignore', 'package.json', 'README.md', 'LICENSE'];
    for (const unwanted of unwantedList) {
      const candidate = path.join(subPath, unwanted);
      if (fs.existsSync(candidate)) {
        fs.unlinkSync(candidate);
      }
    }
  }

  stageReactSubpackage(rootDir, outDir);

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

export function stageLocalPkgNpm(rootDir = DEFAULT_ROOT) {
  const pkgNpmDir = path.join(rootDir, 'lavi-wasm', 'pkg-npm');
  if (!fs.existsSync(pkgNpmDir)) {
    return;
  }
  stageReactSubpackage(rootDir, pkgNpmDir, { flatParentDir: true });
  const pkgJsonPath = path.join(pkgNpmDir, 'package.json');
  if (fs.existsSync(pkgJsonPath)) {
    const pkg = JSON.parse(fs.readFileSync(pkgJsonPath, 'utf8'));
    pkg.name = 'lavi-fsm';
    if (Array.isArray(pkg.files) && !pkg.files.includes('react/')) {
      pkg.files.push('react/');
    }
    pkg.exports = {
      '.': {
        types: './lavi.d.ts',
        default: './lavi.js',
      },
      './react': {
        types: './react/index.d.ts',
        default: './react/index.cjs',
      },
      './package.json': './package.json',
    };
    fs.writeFileSync(pkgJsonPath, JSON.stringify(pkg, null, 2) + '\n', 'utf8');
  }
}

const isMain =
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url));

if (isMain) {
  if (process.argv.includes('--stage-local')) {
    stageLocalPkgNpm();
  } else {
    const skipWasmBuild = process.argv.includes('--skip-wasm-build');
    const res = assembleNpmPackageDir({ skipWasmBuild });
    console.log(`✅ Assembled unified lavi-fsm@${res.manifest.version} npm package at ${res.outDir}`);
  }
}
