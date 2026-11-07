#!/usr/bin/env node
import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');

/**
 * Desktop cross-compilation targets for Server JVM and Python wheels.
 * jvmFolder and libName align 1:1 with NativeLoader.java resolution logic.
 */
export const DESKTOP_TARGETS = [
  {
    triple: 'aarch64-apple-darwin',
    os: 'macos',
    arch: 'aarch64',
    libc: null,
    jvmFolder: 'darwin-aarch64',
    libName: 'liblavi_jvm.dylib',
    pyLibName: 'liblavi_py.dylib'
  },
  {
    triple: 'x86_64-apple-darwin',
    os: 'macos',
    arch: 'x86_64',
    libc: null,
    jvmFolder: 'darwin-x86_64',
    libName: 'liblavi_jvm.dylib',
    pyLibName: 'liblavi_py.dylib'
  },
  {
    triple: 'x86_64-unknown-linux-gnu',
    os: 'linux',
    arch: 'x86_64',
    libc: 'gnu',
    jvmFolder: 'linux-x86_64-gnu',
    libName: 'liblavi_jvm.so',
    pyLibName: 'liblavi_py.so'
  },
  {
    triple: 'aarch64-unknown-linux-gnu',
    os: 'linux',
    arch: 'aarch64',
    libc: 'gnu',
    jvmFolder: 'linux-aarch64-gnu',
    libName: 'liblavi_jvm.so',
    pyLibName: 'liblavi_py.so'
  },
  {
    triple: 'x86_64-unknown-linux-musl',
    os: 'linux',
    arch: 'x86_64',
    libc: 'musl',
    jvmFolder: 'linux-x86_64-musl',
    libName: 'liblavi_jvm.so',
    pyLibName: 'liblavi_py.so'
  },
  {
    triple: 'aarch64-unknown-linux-musl',
    os: 'linux',
    arch: 'aarch64',
    libc: 'musl',
    jvmFolder: 'linux-aarch64-musl',
    libName: 'liblavi_jvm.so',
    pyLibName: 'liblavi_py.so'
  },
  {
    triple: 'x86_64-pc-windows-gnu',
    os: 'windows',
    arch: 'x86_64',
    libc: 'gnu',
    jvmFolder: 'windows-x86_64',
    libName: 'lavi_jvm.dll',
    pyLibName: 'lavi_py.dll'
  }
];

/**
 * Android ABI targets for AAR packaging.
 * 64-bit ABIs enforce 16 KB (0x4000) max-page-size alignment for Android 15+ compliance.
 */
export const ANDROID_TARGETS = [
  {
    abi: 'arm64-v8a',
    triple: 'aarch64-linux-android',
    is64Bit: true,
    rustflags: '-C link-arg=-Wl,-z,max-page-size=16384',
    libName: 'liblavi_jvm.so'
  },
  {
    abi: 'armeabi-v7a',
    triple: 'armv7-linux-androideabi',
    is64Bit: false,
    rustflags: '',
    libName: 'liblavi_jvm.so'
  },
  {
    abi: 'x86_64',
    triple: 'x86_64-linux-android',
    is64Bit: true,
    rustflags: '-C link-arg=-Wl,-z,max-page-size=16384',
    libName: 'liblavi_jvm.so'
  }
];

/**
 * Validates that all ELF LOAD program headers in readelf / llvm-readelf -l output
 * have an alignment of at least 0x4000 (16 KB).
 */
export function verifyElf16KbAlignmentOutput(readelfOutput) {
  if (typeof readelfOutput !== 'string') return false;
  const lines = readelfOutput.split('\n');
  let loadSegmentsFound = 0;

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed.includes('LOAD')) continue;

    const tokens = trimmed.split(/\s+/);
    const loadIdx = tokens.findIndex((t) => t === 'LOAD');
    if (loadIdx === -1) continue;

    const lastToken = tokens[tokens.length - 1];
    let alignVal;
    if (lastToken.startsWith('0x') || lastToken.startsWith('0X')) {
      alignVal = Number.parseInt(lastToken, 16);
    } else {
      alignVal = Number.parseInt(lastToken, 10);
    }

    if (Number.isNaN(alignVal)) continue;

    loadSegmentsFound++;
    if (alignVal < 0x4000) {
      return false;
    }
  }

  return loadSegmentsFound > 0;
}

/**
 * Builds the complete release execution plan for:
 * 1. Desktop JNI binaries via `cargo zigbuild`
 * 2. Desktop Python wheels via `maturin build --zig`
 * 3. Android JNI binaries via `cargo ndk` with 16 KB page-size link flags
 * 4. Android AAR packaging with classes.jar, AndroidManifest.xml, and jni/<abi>/
 */
export function buildReleasePlan(options = {}) {
  const rootDir = options.rootDir || ROOT_DIR;
  const outDir = options.outDir || path.join(rootDir, 'dist');
  const dryRun = options.dryRun ?? false;

  const desktopCommands = [];

  for (const target of DESKTOP_TARGETS) {
    // Desktop JNI for Server JAR
    desktopCommands.push({
      type: 'jvm-desktop',
      triple: target.triple,
      command: 'cargo',
      args: ['zigbuild', '--release', '--package', 'lavi-jvm', '--target', target.triple],
      srcBinary: path.join(rootDir, 'target', target.triple, 'release', target.libName),
      destResource: path.join(
        rootDir,
        'lavi-jvm',
        'src',
        'main',
        'resources',
        'native',
        target.jvmFolder,
        target.libName
      )
    });

    // Python wheels
    desktopCommands.push({
      type: 'py-desktop',
      triple: target.triple,
      command: 'maturin',
      args: [
        'build',
        '--release',
        '-m',
        path.join(rootDir, 'lavi-py', 'Cargo.toml'),
        '--zig',
        '--target',
        target.triple,
        '--out',
        path.join(outDir, 'wheels')
      ],
      outDir: path.join(outDir, 'wheels')
    });
  }

  const androidCommands = [];
  for (const target of ANDROID_TARGETS) {
    androidCommands.push({
      abi: target.abi,
      triple: target.triple,
      is64Bit: target.is64Bit,
      rustflags: target.rustflags,
      command: 'cargo',
      args: ['ndk', '-t', target.abi, 'build', '--release', '--package', 'lavi-jvm'],
      srcBinary: path.join(rootDir, 'target', target.triple, 'release', target.libName),
      aarJniPath: `jni/${target.abi}/${target.libName}`,
      verifyAlignment: target.is64Bit
    });
  }

  const aarManifest = `<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="io.lavi">
</manifest>
`;

  const aarPackage = {
    aarPath: path.join(outDir, 'aar', 'lavi-release.aar'),
    stagingDir: path.join(outDir, 'aar', 'staging'),
    classesJar: path.join(rootDir, 'lavi-jvm', 'build', 'libs', 'lavi-jvm.jar'),
    manifestPath: path.join(outDir, 'aar', 'staging', 'AndroidManifest.xml'),
    manifestContent: aarManifest,
    entries: [
      'AndroidManifest.xml',
      'classes.jar',
      ...ANDROID_TARGETS.map((t) => `jni/${t.abi}/${t.libName}`)
    ]
  };

  return {
    rootDir,
    outDir,
    dryRun,
    desktopCommands,
    androidCommands,
    aarPackage
  };
}

const isMain =
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url));

if (isMain) {
  const plan = buildReleasePlan({ dryRun: true });
  console.log('=== Lavi Single-Host Release Cross-Compilation Plan ===\n');
  console.log(`Desktop Targets (${DESKTOP_TARGETS.length}):`);
  for (const t of DESKTOP_TARGETS) {
    console.log(`  - ${t.triple} -> jvm: native/${t.jvmFolder}/${t.libName}`);
  }
  console.log(`\nAndroid Targets (${ANDROID_TARGETS.length}):`);
  for (const t of ANDROID_TARGETS) {
    const flags = t.rustflags ? ` [${t.rustflags}]` : '';
    console.log(`  - ${t.abi} (${t.triple})${flags}`);
  }
  console.log(`\nAAR Output: ${plan.aarPackage.aarPath}`);
  console.log(`AAR Entries: ${plan.aarPackage.entries.join(', ')}`);
  console.log('\n✅ Plan generated successfully.');
}
