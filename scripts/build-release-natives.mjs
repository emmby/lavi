#!/usr/bin/env node
import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

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
 * Embedded microcontroller targets for unified Arduino IDE & PlatformIO release archive.
 * Maps Arduino/PlatformIO {build.mcu} tokens to ESP Rust target triples.
 */
export const EMBEDDED_TARGETS = [
  {
    mcu: 'esp32',
    triple: 'xtensa-esp32-espidf',
    isRiscv: false,
    libName: 'liblavi_c_cpp.a'
  },
  {
    mcu: 'esp32s2',
    triple: 'xtensa-esp32s2-espidf',
    isRiscv: false,
    libName: 'liblavi_c_cpp.a'
  },
  {
    mcu: 'esp32s3',
    triple: 'xtensa-esp32s3-espidf',
    isRiscv: false,
    libName: 'liblavi_c_cpp.a'
  },
  {
    mcu: 'esp32c3',
    triple: 'riscv32imc-esp-espidf',
    isRiscv: true,
    libName: 'liblavi_c_cpp.a'
  },
  {
    mcu: 'esp32c6',
    triple: 'riscv32imac-esp-espidf',
    isRiscv: true,
    libName: 'liblavi_c_cpp.a'
  },
  {
    mcu: 'esp32h2',
    triple: 'riscv32imac-esp-espidf',
    isRiscv: true,
    libName: 'liblavi_c_cpp.a'
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

  let pkgVersion = '0.1.0';
  const pkgJsonPath = path.join(rootDir, 'package.json');
  if (fs.existsSync(pkgJsonPath)) {
    try {
      pkgVersion = JSON.parse(fs.readFileSync(pkgJsonPath, 'utf8')).version || '0.1.0';
    } catch {
      pkgVersion = '0.1.0';
    }
  }

  const aarPackage = {
    aarPath: path.join(outDir, 'aar', 'lavi-release.aar'),
    stagingDir: path.join(outDir, 'aar', 'staging'),
    classesJar: path.join(rootDir, 'lavi-jvm', 'build', 'libs', `lavi-jvm-${pkgVersion}.jar`),
    manifestPath: path.join(outDir, 'aar', 'staging', 'AndroidManifest.xml'),
    manifestContent: aarManifest,
    entries: [
      'AndroidManifest.xml',
      'classes.jar',
      ...ANDROID_TARGETS.map((t) => `jni/${t.abi}/${t.libName}`)
    ]
  };

  const embeddedCommands = [];
  for (const target of EMBEDDED_TARGETS) {
    embeddedCommands.push({
      mcu: target.mcu,
      triple: target.triple,
      isRiscv: target.isRiscv,
      command: 'cargo',
      args: [
        'build',
        '--manifest-path',
        path.join(rootDir, 'lavi-c-cpp', 'Cargo.toml'),
        '--release',
        '-Z',
        'build-std=std,panic_abort',
        '--target',
        target.triple
      ],
      srcBinary: path.join(rootDir, 'target', target.triple, 'release', target.libName),
      zipEntryPath: `lavi/src/${target.mcu}/${target.libName}`
    });
  }

  const embeddedPackage = {
    zipPath: path.join(outDir, 'embedded', 'lavi-embedded.zip'),
    stagingDir: path.join(outDir, 'embedded', 'staging'),
    entries: [
      'lavi/library.properties',
      'lavi/library.json',
      'lavi/build_rust.py',
      'lavi/LICENSE',
      'lavi/README.md',
      'lavi/src/lavi.h',
      'lavi/src/lavi.hpp',
      ...EMBEDDED_TARGETS.map((t) => `lavi/src/${t.mcu}/${t.libName}`),
      'lavi/examples/Stoplight/Stoplight.ino'
    ]
  };

  return {
    rootDir,
    outDir,
    dryRun,
    desktopCommands,
    androidCommands,
    aarPackage,
    embeddedCommands,
    embeddedPackage
  };
}

const CRC32_TABLE = new Int32Array(256);
for (let i = 0; i < 256; i++) {
  let c = i;
  for (let j = 0; j < 8; j++) {
    c = (c & 1) ? (0xedb88320 ^ (c >>> 1)) : (c >>> 1);
  }
  CRC32_TABLE[i] = c;
}

export function crc32(buffer) {
  let c = -1;
  for (let i = 0; i < buffer.length; i++) {
    c = CRC32_TABLE[(c ^ buffer[i]) & 0xff] ^ (c >>> 8);
  }
  return (c ^ -1) >>> 0;
}

export function createZipArchive(entries) {
  const localHeaders = [];
  const centralHeaders = [];
  let offset = 0;

  for (const entry of entries) {
    const nameBuf = Buffer.from(entry.path, 'utf8');
    const dataBuf = Buffer.isBuffer(entry.data) ? entry.data : Buffer.from(entry.data || '');
    const crc = crc32(dataBuf);
    const uncompressedSize = dataBuf.length;
    const compressedSize = uncompressedSize;

    // Local file header: 30 bytes + name length
    const localHeader = Buffer.alloc(30 + nameBuf.length);
    localHeader.writeUInt32LE(0x04034b50, 0); // PK\x03\x04
    localHeader.writeUInt16LE(20, 4);         // version 2.0
    localHeader.writeUInt16LE(0x0800, 6);     // UTF-8 flag
    localHeader.writeUInt16LE(0, 8);          // STORED (method 0)
    localHeader.writeUInt16LE(0, 10);
    localHeader.writeUInt16LE(0x21, 12);
    localHeader.writeUInt32LE(crc, 14);
    localHeader.writeUInt32LE(compressedSize, 18);
    localHeader.writeUInt32LE(uncompressedSize, 22);
    localHeader.writeUInt16LE(nameBuf.length, 26);
    localHeader.writeUInt16LE(0, 28);
    nameBuf.copy(localHeader, 30);

    localHeaders.push(localHeader, dataBuf);

    // Central directory header: 46 bytes + name length
    const centralHeader = Buffer.alloc(46 + nameBuf.length);
    centralHeader.writeUInt32LE(0x02014b50, 0); // PK\x01\x02
    centralHeader.writeUInt16LE(0x0314, 4);     // version made by (UNIX 2.0)
    centralHeader.writeUInt16LE(20, 6);         // version needed
    centralHeader.writeUInt16LE(0x0800, 8);     // UTF-8
    centralHeader.writeUInt16LE(0, 10);         // STORED
    centralHeader.writeUInt16LE(0, 12);
    centralHeader.writeUInt16LE(0x21, 14);
    centralHeader.writeUInt32LE(crc, 16);
    centralHeader.writeUInt32LE(compressedSize, 20);
    centralHeader.writeUInt32LE(uncompressedSize, 24);
    centralHeader.writeUInt16LE(nameBuf.length, 28);
    centralHeader.writeUInt16LE(0, 30);
    centralHeader.writeUInt16LE(0, 32);
    centralHeader.writeUInt16LE(0, 34);
    centralHeader.writeUInt16LE(0, 36);
    centralHeader.writeUInt32LE(0x81a40000, 38);// regular file 0644
    centralHeader.writeUInt32LE(offset, 42);
    nameBuf.copy(centralHeader, 46);

    centralHeaders.push(centralHeader);

    offset += localHeader.length + dataBuf.length;
  }

  const centralDirOffset = offset;
  let centralDirSize = 0;
  for (const ch of centralHeaders) {
    centralDirSize += ch.length;
  }

  // End of central directory record: 22 bytes
  const eocd = Buffer.alloc(22);
  eocd.writeUInt32LE(0x06054b50, 0);          // PK\x05\x06
  eocd.writeUInt16LE(0, 4);
  eocd.writeUInt16LE(0, 6);
  eocd.writeUInt16LE(entries.length, 8);
  eocd.writeUInt16LE(entries.length, 10);
  eocd.writeUInt32LE(centralDirSize, 12);
  eocd.writeUInt32LE(centralDirOffset, 16);
  eocd.writeUInt16LE(0, 20);

  return Buffer.concat([...localHeaders, ...centralHeaders, eocd]);
}

/**
 * Packages an Android AAR zip archive containing:
 * - AndroidManifest.xml
 * - classes.jar
 * - R.txt
 * - jni/<abi>/liblavi_jvm.so
 */
export function packageAndroidAar({ classesJarPath, jniLibsDir, outputAarPath, packageName = 'io.lavi' }) {
  const entries = [];

  // 1. AndroidManifest.xml
  const manifestXml = `<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="${packageName}">
</manifest>
`;
  entries.push({ path: 'AndroidManifest.xml', data: Buffer.from(manifestXml, 'utf8') });

  // 2. classes.jar
  if (classesJarPath && fs.existsSync(classesJarPath)) {
    entries.push({ path: 'classes.jar', data: fs.readFileSync(classesJarPath) });
  } else {
    // Generate valid empty jar (valid empty zip archive)
    const emptyJar = createZipArchive([]);
    entries.push({ path: 'classes.jar', data: emptyJar });
  }

  // 3. R.txt
  entries.push({ path: 'R.txt', data: Buffer.alloc(0) });

  // 4. JNI shared libraries
  if (jniLibsDir && fs.existsSync(jniLibsDir)) {
    const stack = [jniLibsDir];
    while (stack.length > 0) {
      const dir = stack.pop();
      const files = fs.readdirSync(dir, { withFileTypes: true });
      for (const file of files) {
        const fullPath = path.join(dir, file.name);
        if (file.isDirectory()) {
          stack.push(fullPath);
        } else if (file.name.endsWith('.so')) {
          const rel = path.relative(jniLibsDir, fullPath).replace(/\\/g, '/');
          const entryPath = rel.startsWith('jni/') ? rel : `jni/${rel}`;
          entries.push({ path: entryPath, data: fs.readFileSync(fullPath) });
        }
      }
    }
  }

  const zipBuf = createZipArchive(entries);
  fs.mkdirSync(path.dirname(outputAarPath), { recursive: true });
  fs.writeFileSync(outputAarPath, zipBuf);

  return {
    aarPath: outputAarPath,
    entries: entries.map((e) => e.path)
  };
}

/**
 * Packages a unified Arduino IDE & PlatformIO zip archive containing:
 * - lavi/library.properties
 * - lavi/library.json
 * - lavi/build_rust.py
 * - lavi/LICENSE
 * - lavi/README.md
 * - lavi/src/lavi.h & lavi/src/lavi.hpp
 * - lavi/src/<mcu>/liblavi_c_cpp.a
 * - lavi/examples/Stoplight/Stoplight.ino
 */
export function packageEmbeddedZip({ rootDir = ROOT_DIR, prebuiltLibsDir, outputZipPath }) {
  const entries = [];
  const cCppDir = path.join(rootDir, 'lavi-c-cpp');

  const staticFiles = [
    { zipPath: 'lavi/library.properties', srcPath: path.join(cCppDir, 'library.properties') },
    { zipPath: 'lavi/library.json', srcPath: path.join(cCppDir, 'library.json') },
    { zipPath: 'lavi/build_rust.py', srcPath: path.join(cCppDir, 'build_rust.py') },
    { zipPath: 'lavi/LICENSE', srcPath: path.join(rootDir, 'LICENSE') },
    { zipPath: 'lavi/README.md', srcPath: path.join(rootDir, 'README.md') },
    { zipPath: 'lavi/src/lavi.h', srcPath: path.join(cCppDir, 'include', 'lavi.h') },
    { zipPath: 'lavi/src/lavi.hpp', srcPath: path.join(cCppDir, 'include', 'lavi.hpp') },
    {
      zipPath: 'lavi/examples/Stoplight/Stoplight.ino',
      srcPath: path.join(rootDir, 'examples', 'esp32', 'Stoplight', 'Stoplight.ino')
    }
  ];

  for (const item of staticFiles) {
    if (!fs.existsSync(item.srcPath)) {
      throw new Error(`Missing required embedded static file for zip archive: ${item.srcPath}`);
    }
    entries.push({ path: item.zipPath, data: fs.readFileSync(item.srcPath) });
  }

  if (prebuiltLibsDir && fs.existsSync(prebuiltLibsDir)) {
    const stack = [prebuiltLibsDir];
    while (stack.length > 0) {
      const dir = stack.pop();
      const files = fs.readdirSync(dir, { withFileTypes: true });
      for (const file of files) {
        const fullPath = path.join(dir, file.name);
        if (file.isDirectory()) {
          stack.push(fullPath);
        } else if (file.name.endsWith('.a')) {
          const rel = path.relative(prebuiltLibsDir, fullPath).replace(/\\/g, '/');
          const entryPath = rel.startsWith('lavi/src/')
            ? rel
            : rel.startsWith('src/')
              ? `lavi/${rel}`
              : `lavi/src/${rel}`;
          entries.push({ path: entryPath, data: fs.readFileSync(fullPath) });
        }
      }
    }
  }

  const zipBuf = createZipArchive(entries);
  fs.mkdirSync(path.dirname(outputZipPath), { recursive: true });
  fs.writeFileSync(outputZipPath, zipBuf);

  return {
    zipPath: outputZipPath,
    entries: entries.map((e) => e.path)
  };
}

/**
 * Executes commands in the release plan via child_process.spawnSync when
 * --execute or dryRun: false is specified.
 */
export function executeReleasePlan(plan, options = {}) {
  const isExecute = options.execute ?? (!plan.dryRun && !(options.dryRun ?? false));
  if (!isExecute) {
    return { executed: false, commandsRun: 0, plan };
  }

  let commandsRun = 0;

  // 1. Desktop compilation
  for (const cmd of plan.desktopCommands) {
    if (options.verbose) {
      console.log(`Executing desktop command: ${cmd.command} ${cmd.args.join(' ')}`);
    }
    const res = spawnSync(cmd.command, cmd.args, {
      stdio: 'inherit',
      cwd: plan.rootDir,
      env: process.env
    });
    if (res.status !== 0) {
      throw new Error(`Command failed with exit code ${res.status}: ${cmd.command} ${cmd.args.join(' ')}`);
    }
    commandsRun++;

    if (cmd.destResource && cmd.srcBinary && fs.existsSync(cmd.srcBinary)) {
      fs.mkdirSync(path.dirname(cmd.destResource), { recursive: true });
      fs.copyFileSync(cmd.srcBinary, cmd.destResource);
    }
  }

  // 2. Android compilation
  for (const cmd of plan.androidCommands) {
    if (options.verbose) {
      console.log(`Executing Android build: ${cmd.command} ${cmd.args.join(' ')}`);
    }
    const env = { ...process.env };
    if (cmd.rustflags) {
      env.RUSTFLAGS = cmd.rustflags;
    }
    const res = spawnSync(cmd.command, cmd.args, {
      stdio: 'inherit',
      cwd: plan.rootDir,
      env
    });
    if (res.status !== 0) {
      throw new Error(`Command failed with exit code ${res.status}: ${cmd.command} ${cmd.args.join(' ')}`);
    }
    commandsRun++;

    if (cmd.verifyAlignment && fs.existsSync(cmd.srcBinary)) {
      const readelf = spawnSync('llvm-readelf', ['-l', cmd.srcBinary], { encoding: 'utf8' });
      if (readelf.status === 0 && !verifyElf16KbAlignmentOutput(readelf.stdout)) {
        throw new Error(`ELF segment alignment verification failed for 16 KB pages: ${cmd.srcBinary}`);
      }
    }
  }

  // 3. Package AAR
  const stagingJni = path.join(plan.outDir, 'aar', 'staging', 'jni');
  for (const cmd of plan.androidCommands) {
    if (fs.existsSync(cmd.srcBinary)) {
      const targetDir = path.join(stagingJni, cmd.abi);
      fs.mkdirSync(targetDir, { recursive: true });
      fs.copyFileSync(cmd.srcBinary, path.join(targetDir, 'liblavi_jvm.so'));
    }
  }

  packageAndroidAar({
    classesJarPath: plan.aarPackage.classesJar,
    jniLibsDir: stagingJni,
    outputAarPath: plan.aarPackage.aarPath,
    packageName: 'io.lavi'
  });

  // 4. Embedded MCU compilation & unified Arduino/PlatformIO zip packaging (when --include-embedded or includeEmbedded is set)
  if (options.includeEmbedded) {
    const stagingEmbedded = path.join(plan.outDir, 'embedded', 'staging', 'src');
    for (const cmd of plan.embeddedCommands || []) {
      if (options.verbose) {
        console.log(`Executing embedded MCU build (${cmd.mcu}): ${cmd.command} ${cmd.args.join(' ')}`);
      }
      const res = spawnSync(cmd.command, cmd.args, {
        stdio: 'inherit',
        cwd: plan.rootDir,
        env: process.env
      });
      if (res.status !== 0) {
        throw new Error(`Command failed with exit code ${res.status}: ${cmd.command} ${cmd.args.join(' ')}`);
      }
      commandsRun++;

      if (fs.existsSync(cmd.srcBinary)) {
        const targetDir = path.join(stagingEmbedded, cmd.mcu);
        fs.mkdirSync(targetDir, { recursive: true });
        fs.copyFileSync(cmd.srcBinary, path.join(targetDir, 'liblavi_c_cpp.a'));
      }
    }

    if (plan.embeddedPackage) {
      packageEmbeddedZip({
        rootDir: plan.rootDir,
        prebuiltLibsDir: stagingEmbedded,
        outputZipPath: plan.embeddedPackage.zipPath
      });
    }
  }

  return {
    executed: true,
    commandsRun,
    plan
  };
}

const isMain =
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url));

if (isMain) {
  const args = process.argv.slice(2);
  const shouldExecute = args.includes('--execute');
  const includeEmbedded = args.includes('--include-embedded');
  const plan = buildReleasePlan({ dryRun: !shouldExecute });
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
  console.log(`\nEmbedded MCU Targets (${EMBEDDED_TARGETS.length}):`);
  for (const t of EMBEDDED_TARGETS) {
    console.log(`  - ${t.mcu} (${t.triple}) -> lavi/src/${t.mcu}/${t.libName}`);
  }
  console.log(`Embedded Archive Output: ${plan.embeddedPackage.zipPath}`);

  if (shouldExecute) {
    console.log('\n🚀 Executing release compilation plan...');
    executeReleasePlan(plan, { execute: true, includeEmbedded, verbose: true });
    console.log('\n✅ Release compilation and packaging completed successfully.');
  } else {
    console.log('\n✅ Plan generated successfully (dry-run). Pass --execute to run.');
  }
}
