// @vitest-environment happy-dom
import { describe, it, expect } from 'vitest';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import * as React from 'react';
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import {
  DESKTOP_TARGETS,
  ANDROID_TARGETS,
  EMBEDDED_TARGETS,
  APPLE_XCFRAMEWORK_TARGETS,
  buildReleasePlan,
  verifyElf16KbAlignmentOutput,
  packageAndroidAar,
  packageEmbeddedZip,
  packageAppleXcframeworkZip,
  executeReleasePlan,
} from '../scripts/build-release-natives.mjs';
import {
  REACT_NATIVE_RUSTFLAGS,
  createNpmPackageManifest,
  patchReactNativeWasm2JsDir,
  resolveWasm2JsCommand,
  assembleNpmPackageDir,
} from '../scripts/build-npm-package.mjs';
import { readAllManifestVersions, checkVersions } from '../scripts/sync-version.mjs';
import { createMachine, assign, SimulatedClock } from '../lavi-wasm/pkg-npm/lavi.js';
import { useMachine, useActor, useActorRef, useSelector } from '../lavi-wasm/pkg-npm/react/index.cjs';

const requireCjs = createRequire(import.meta.url);

describe('build-release-natives', () => {
  it('includes all required desktop gnu, musl, darwin, and windows targets', () => {
    const triples = DESKTOP_TARGETS.map((t: any) => t.triple);
    expect(triples).toContain('x86_64-unknown-linux-gnu');
    expect(triples).toContain('aarch64-unknown-linux-gnu');
    expect(triples).toContain('x86_64-unknown-linux-musl');
    expect(triples).toContain('aarch64-unknown-linux-musl');
    expect(triples).toContain('aarch64-apple-darwin');
    expect(triples).toContain('x86_64-apple-darwin');
    expect(triples).toContain('x86_64-pc-windows-gnu');
  });

  it('enforces 16 KB page alignment link flags on 64-bit Android targets and -crt-static on musl JVM targets', () => {
    const plan = buildReleasePlan({ dryRun: true });
    const arm64Cmd = plan.androidCommands.find((c: any) => c.abi === 'arm64-v8a');
    expect(arm64Cmd.rustflags).toContain('-C link-arg=-Wl,-z,max-page-size=16384');
    const x86Cmd = plan.androidCommands.find((c: any) => c.abi === 'x86_64');
    expect(x86Cmd.rustflags).toContain('-C link-arg=-Wl,-z,max-page-size=16384');

    const muslJvmCmds = plan.desktopCommands.filter(
      (c: any) => c.type === 'jvm-desktop' && c.triple.includes('-musl')
    );
    expect(muslJvmCmds.length).toBe(2);
    for (const cmd of muslJvmCmds) {
      expect(cmd.rustflags).toContain('-C target-feature=-crt-static');
    }
  });

  it('validates ELF LOAD segment alignment is at least 0x4000 (16 KB)', () => {
    const validReadelf = '  LOAD           0x000000 0x0000000000000000 0x0000000000000000 0x012340 0x012340 R E 0x4000';
    const invalidReadelf = '  LOAD           0x000000 0x0000000000000000 0x0000000000000000 0x012340 0x012340 R E 0x1000';
    expect(verifyElf16KbAlignmentOutput(validReadelf)).toBe(true);
    expect(verifyElf16KbAlignmentOutput(invalidReadelf)).toBe(false);
  });

  it('packages Android AAR in .tmp/ and validates zip header PK\\x03\\x04 and entries', () => {
    const tmpDir = path.resolve('.tmp', 'aar-test-' + Date.now());
    const stagingDir = path.join(tmpDir, 'staging');
    const jniDir = path.join(stagingDir, 'jni');
    const arm64Dir = path.join(jniDir, 'arm64-v8a');
    const x86_64Dir = path.join(jniDir, 'x86_64');
    fs.mkdirSync(arm64Dir, { recursive: true });
    fs.mkdirSync(x86_64Dir, { recursive: true });

    // Create mock classes.jar and native .so files
    const mockJarPath = path.join(stagingDir, 'classes.jar');
    fs.writeFileSync(mockJarPath, Buffer.from('mock-classes-jar-content'));
    fs.writeFileSync(path.join(arm64Dir, 'liblavi_jvm.so'), Buffer.from('dummy-arm64-so'));
    fs.writeFileSync(path.join(x86_64Dir, 'liblavi_jvm.so'), Buffer.from('dummy-x86_64-so'));

    const outputAarPath = path.join(tmpDir, 'output', 'test-lavi.aar');

    const result = packageAndroidAar({
      classesJarPath: mockJarPath,
      jniLibsDir: jniDir,
      outputAarPath,
      packageName: 'io.lavi.test',
    });

    expect(fs.existsSync(outputAarPath)).toBe(true);
    const aarBuffer = fs.readFileSync(outputAarPath);

    // Verify valid ZIP local file header PK\x03\x04
    expect(aarBuffer[0]).toBe(0x50); // 'P'
    expect(aarBuffer[1]).toBe(0x4b); // 'K'
    expect(aarBuffer[2]).toBe(0x03);
    expect(aarBuffer[3]).toBe(0x04);

    // Verify expected entries are present
    expect(result.entries).toContain('AndroidManifest.xml');
    expect(result.entries).toContain('classes.jar');
    expect(result.entries).toContain('R.txt');
    expect(result.entries).toContain('jni/arm64-v8a/liblavi_jvm.so');
    expect(result.entries).toContain('jni/x86_64/liblavi_jvm.so');
  });

  it('executeReleasePlan returns dry-run when execute is not specified', () => {
    const plan = buildReleasePlan({ dryRun: true });
    const result = executeReleasePlan(plan, { dryRun: true });
    expect(result.executed).toBe(false);
    expect(result.commandsRun).toBe(0);
  });

  it('includes all 6 ESP32 and ARM Cortex-M (including hard/softfp FPU variants) targets in EMBEDDED_TARGETS and buildReleasePlan', () => {
    const mcus = EMBEDDED_TARGETS.map((t: any) => t.mcu);
    expect(mcus).toEqual([
      'esp32',
      'esp32s2',
      'esp32s3',
      'esp32c3',
      'esp32c6',
      'esp32h2',
      'cortex-m0plus',
      'cortex-m3',
      'cortex-m4',
      'cortex-m4/fpv4-sp-d16-hard',
      'cortex-m4/fpv4-sp-d16-softfp',
      'cortex-m7',
      'cortex-m7/fpv5-d16-hard',
      'cortex-m7/fpv5-sp-d16-hard',
      'cortex-m7/fpv5-d16-softfp',
      'cortex-m7/fpv5-sp-d16-softfp',
      'cortex-m33',
      'cortex-m33/fpv5-sp-d16-hard',
      'cortex-m33/fpv5-sp-d16-softfp',
    ]);

    const plan = buildReleasePlan({ dryRun: true });
    expect(plan.embeddedCommands.length).toBe(19);
    const cm4Hard = plan.embeddedCommands.find((c: any) => c.mcu === 'cortex-m4');
    expect(cm4Hard.triple).toBe('thumbv7em-none-eabihf');
    expect(cm4Hard.args).toContain('build-std=core,alloc,panic_abort');
    const cm4Softfp = plan.embeddedCommands.find((c: any) => c.mcu === 'cortex-m4/fpv4-sp-d16-softfp');
    expect(cm4Softfp.triple).toBe('thumbv7em-none-eabi');
    const cm33Softfp = plan.embeddedCommands.find((c: any) => c.mcu === 'cortex-m33/fpv5-sp-d16-softfp');
    expect(cm33Softfp.triple).toBe('thumbv8m.main-none-eabi');
    expect(plan.embeddedPackage.zipPath).toContain('lavi-embedded.zip');
  });

  it('packages unified Arduino IDE and PlatformIO zip archive in .tmp/ with precompiled .a and manifests', () => {
    const tmpDir = path.resolve('.tmp', 'embedded-zip-test-' + Date.now());
    const prebuiltDir = path.join(tmpDir, 'prebuilt');
    const esp32Dir = path.join(prebuiltDir, 'esp32');
    const esp32s3Dir = path.join(prebuiltDir, 'esp32s3');
    fs.mkdirSync(esp32Dir, { recursive: true });
    fs.mkdirSync(esp32s3Dir, { recursive: true });

    fs.writeFileSync(path.join(esp32Dir, 'liblavi_c_cpp.a'), Buffer.from('!<arch>\ndummy-esp32-a'));
    fs.writeFileSync(path.join(esp32s3Dir, 'liblavi_c_cpp.a'), Buffer.from('!<arch>\ndummy-esp32s3-a'));

    const outputZipPath = path.join(tmpDir, 'output', 'lavi-embedded.zip');
    const result = packageEmbeddedZip({
      prebuiltLibsDir: prebuiltDir,
      outputZipPath,
    });

    expect(fs.existsSync(outputZipPath)).toBe(true);
    const zipBuffer = fs.readFileSync(outputZipPath);
    expect(zipBuffer[0]).toBe(0x50); // 'P'
    expect(zipBuffer[1]).toBe(0x4b); // 'K'
    expect(zipBuffer[2]).toBe(0x03);
    expect(zipBuffer[3]).toBe(0x04);

    expect(result.entries).toContain('lavi/library.properties');
    expect(result.entries).toContain('lavi/library.json');
    expect(result.entries).toContain('lavi/build_rust.py');
    expect(result.entries).toContain('lavi/LICENSE');
    expect(result.entries).toContain('lavi/README.md');
    expect(result.entries).toContain('lavi/src/lavi.h');
    expect(result.entries).toContain('lavi/src/lavi.hpp');
    expect(result.entries).toContain('lavi/src/esp32/liblavi_c_cpp.a');
    expect(result.entries).toContain('lavi/src/esp32s3/liblavi_c_cpp.a');
    expect(result.entries).toContain('lavi/examples/Stoplight/Stoplight.ino');

    // Verify missing static files throw an explicit error
    expect(() =>
      packageEmbeddedZip({
        rootDir: path.join(tmpDir, 'nonexistent-root'),
        prebuiltLibsDir: prebuiltDir,
        outputZipPath: path.join(tmpDir, 'output', 'should-fail.zip'),
      })
    ).toThrow(/Missing required embedded static file/);
  });

  it('includes Apple XCFramework targets and packages LaviFFI.xcframework.zip in .tmp/', () => {
    const triples = APPLE_XCFRAMEWORK_TARGETS.map((t: any) => t.triple);
    expect(triples).toEqual([
      'aarch64-apple-darwin',
      'x86_64-apple-darwin',
      'aarch64-apple-ios',
      'aarch64-apple-ios-sim',
      'x86_64-apple-ios',
    ]);

    const plan = buildReleasePlan({ dryRun: true });
    expect(plan.appleCommands.length).toBe(5);
    expect(plan.applePackage.zipPath).toContain('LaviFFI.xcframework.zip');

    const tmpDir = path.resolve('.tmp', 'apple-xcframework-test-' + Date.now());
    const slicesDir = path.join(tmpDir, 'slices');
    for (const slice of ['ios-arm64', 'ios-arm64_x86_64-simulator', 'macos-arm64_x86_64']) {
      const sDir = path.join(slicesDir, slice);
      fs.mkdirSync(sDir, { recursive: true });
      fs.writeFileSync(path.join(sDir, 'liblavi_c_cpp.a'), Buffer.from(`!<arch>\n${slice}`));
    }

    const outputZipPath = path.join(tmpDir, 'output', 'LaviFFI.xcframework.zip');
    const result = packageAppleXcframeworkZip({
      slicesDir,
      outputZipPath,
    });

    expect(fs.existsSync(outputZipPath)).toBe(true);
    expect(result.entries).toContain('LaviFFI.xcframework/Info.plist');
    expect(result.entries).toContain('LaviFFI.xcframework/ios-arm64/liblavi_c_cpp.a');
    expect(result.entries).toContain('LaviFFI.xcframework/ios-arm64/Headers/lavi.h');
    expect(result.entries).toContain('LaviFFI.xcframework/ios-arm64/Headers/module.modulemap');
    expect(result.entries).toContain('LaviFFI.xcframework/ios-arm64_x86_64-simulator/liblavi_c_cpp.a');
    expect(result.entries).toContain('LaviFFI.xcframework/macos-arm64_x86_64/liblavi_c_cpp.a');
  });

  it('tracks Dart pubspec.yaml manifests in sync-version.mjs', () => {
    const versions = readAllManifestVersions();
    expect(versions['lavi-dart/pubspec.yaml']).toBe('0.2.0');
    expect(versions['examples/dart/pubspec.yaml']).toBe('0.2.0');
    const check = checkVersions();
    expect(check.ok).toBe(true);
  });

  it('configures react-native wasm2js and react hooks exports in createNpmPackageManifest and patchReactNativeWasm2JsDir', () => {
    const manifest = createNpmPackageManifest('0.2.0');
    expect(manifest['react-native']).toBe('./react-native/lavi.js');
    expect(manifest.files).toContain('react-native/');
    expect(manifest.files).toContain('react/');
    expect(manifest.peerDependencies).toEqual({ react: '>=18.0.0' });
    expect(manifest.peerDependenciesMeta).toEqual({ react: { optional: true } });
    expect(manifest.exports['.']['react-native']).toBe('./react-native/lavi.js');
    expect(manifest.exports['./react-native']).toEqual({
      types: './react-native/lavi.d.ts',
      import: './react-native/lavi.js',
      default: './react-native/lavi.js',
    });
    expect(manifest.exports['./react']).toEqual({
      types: './react/index.d.ts',
      'react-native': './react/index.react-native.js',
      node: './react/index.cjs',
      browser: './react/index.js',
      import: './react/index.js',
      default: './react/index.cjs',
    });

    const tmpDir = path.resolve('.tmp', 'rn-patch-test-' + Date.now());
    const rnDir = path.join(tmpDir, 'react-native');
    fs.mkdirSync(rnDir, { recursive: true });
    fs.writeFileSync(
      path.join(rnDir, 'lavi_bg.wasm.js'),
      `import * as __wbindgen_placeholder__ from '__wbindgen_placeholder__';\nfunction asmFunc(imports) { return { memory: { buffer: new ArrayBuffer(8) } }; }\nvar retasmFunc = asmFunc({\n  "__wbindgen_placeholder__": __wbindgen_placeholder__,\n});\nexport var memory = retasmFunc.memory;\n`,
      'utf8'
    );
    fs.writeFileSync(
      path.join(rnDir, 'lavi.js'),
      `let imports = {};\nlet cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });\ncachedTextDecoder.decode();\nfunction decodeText(ptr, len) {\n    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));\n}\n\nconst cachedTextEncoder = new TextEncoder();\n\nif (!('encodeInto' in cachedTextEncoder)) {\n    cachedTextEncoder.encodeInto = function (arg, view) {\n        const buf = cachedTextEncoder.encode(arg);\n        view.set(buf);\n        return {\n            read: arg.length,\n            written: buf.length\n        };\n    }\n}\nexports.__wbg_new_no_args_123 = function(arg0, arg1) {\n    const ret = new Function(getStringFromWasm0(arg0, arg1));\n    return ret;\n};\nconst wasmPath = \`\${__dirname}/lavi_bg.wasm\`;\nconst wasmBytes = require('fs').readFileSync(wasmPath);\nconst wasmModule = new WebAssembly.Module(wasmBytes);\nconst wasm = exports.__wasm = new WebAssembly.Instance(wasmModule, imports).exports;\n`,
      'utf8'
    );
    fs.writeFileSync(path.join(rnDir, 'lavi_bg.wasm'), Buffer.from('dummy'));

    patchReactNativeWasm2JsDir(rnDir);

    expect(fs.existsSync(path.join(rnDir, 'lavi_bg.wasm'))).toBe(false);
    expect(fs.existsSync(path.join(rnDir, 'index.js'))).toBe(true);
    expect(fs.existsSync(path.join(rnDir, 'index.d.ts'))).toBe(true);
    expect(fs.existsSync(path.join(rnDir, 'package.json'))).toBe(true);
    const patchedWasmJs = fs.readFileSync(path.join(rnDir, 'lavi_bg.wasm.js'), 'utf8');
    expect(patchedWasmJs).not.toContain('import * as __wbindgen_placeholder__');
    expect(patchedWasmJs).toContain('module.exports = { asmFunc };');

    const patchedLaviJs = fs.readFileSync(path.join(rnDir, 'lavi.js'), 'utf8');
    expect(patchedLaviJs).not.toContain('new WebAssembly.Instance');
    expect(patchedLaviJs).not.toContain('readFileSync');
    expect(patchedLaviJs).not.toContain('new Function(');
    expect(patchedLaviJs).toContain('__laviUtf8DecodeFallback');
    expect(patchedLaviJs).toContain("require('./lavi_bg.wasm.js')");

    const pkgOutDir = path.join(tmpDir, 'npm-out');
    assembleNpmPackageDir({ outDir: pkgOutDir, skipWasmBuild: true });
    expect(fs.existsSync(path.join(pkgOutDir, 'react', 'index.cjs'))).toBe(true);
    expect(fs.existsSync(path.join(pkgOutDir, 'react', 'index.js'))).toBe(true);
    expect(fs.existsSync(path.join(pkgOutDir, 'react', 'index.react-native.js'))).toBe(true);
    expect(fs.existsSync(path.join(pkgOutDir, 'react', 'index.native.js'))).toBe(true);
    expect(fs.existsSync(path.join(pkgOutDir, 'react', 'package.json'))).toBe(true);
    expect(fs.existsSync(path.join(pkgOutDir, 'react', 'index.d.ts'))).toBe(true);
    expect(fs.existsSync(path.join(pkgOutDir, 'react', 'index.d.cts'))).toBe(true);
  });

  it('executes the react-native wasm2js bundle without WebAssembly and with pure-JS UTF-8 fallback', () => {
    let rnEntryPath = path.resolve('dist', 'npm', 'react-native', 'lavi.js');
    if (!fs.existsSync(rnEntryPath)) {
      const tmpRnDir = path.resolve('.tmp', 'rn-wasm2js-runtime-' + Date.now());
      fs.mkdirSync(tmpRnDir, { recursive: true });
      const wasmCrateDir = path.resolve('lavi-wasm');
      const env = {
        ...process.env,
        RUSTFLAGS: process.env.RUSTFLAGS
          ? `${process.env.RUSTFLAGS} ${REACT_NATIVE_RUSTFLAGS}`
          : REACT_NATIVE_RUSTFLAGS,
      };
      const wpRes = spawnSync(
        'wasm-pack',
        ['build', '--release', '--target', 'nodejs', '--out-dir', tmpRnDir, '--out-name', 'lavi'],
        { cwd: wasmCrateDir, stdio: 'inherit', env }
      );
      expect(wpRes.status).toBe(0);
      const { command, prefixArgs } = resolveWasm2JsCommand();
      const w2jRes = spawnSync(
        command,
        [
          ...prefixArgs,
          '--enable-bulk-memory',
          '--enable-sign-ext',
          '--enable-mutable-globals',
          '-O3',
          path.join(tmpRnDir, 'lavi_bg.wasm'),
          '-o',
          path.join(tmpRnDir, 'lavi_bg.wasm.js'),
        ],
        { cwd: wasmCrateDir, stdio: 'inherit' }
      );
      expect(w2jRes.status).toBe(0);
      patchReactNativeWasm2JsDir(tmpRnDir);
      rnEntryPath = path.join(tmpRnDir, 'lavi.js');
    }

    const origWasm = globalThis.WebAssembly;
    // @ts-ignore
    globalThis.__LAVI_FORCE_UTF8_FALLBACK__ = true;
    // @ts-ignore
    delete globalThis.WebAssembly;

    try {
      delete requireCjs.cache[requireCjs.resolve(rnEntryPath)];
      const rnLavi = requireCjs(rnEntryPath);
      const clock = new rnLavi.SimulatedClock();
      const machine = rnLavi.createMachine({
        id: 'rnHermesMachine',
        initial: 'idle',
        context: { count: 0, label: 'שלום 状态机 🦁' },
        states: {
          idle: {
            on: {
              START: {
                target: 'running',
                actions: rnLavi.assign({
                  count: ({ context, event }: any) => context.count + (event.step ?? 1),
                  label: ({ event }: any) => event.text ?? 'שלום 状态机 🦁',
                }),
              },
            },
          },
          running: {
            after: {
              250: 'finished',
            },
          },
          finished: {
            type: 'final',
          },
        },
      });

      const actor = rnLavi.createActor(machine, { clock }).start();
      expect(actor.getSnapshot().value).toBe('idle');
      expect(actor.getSnapshot().context).toEqual({
        count: 0,
        label: 'שלום 状态机 🦁',
      });

      actor.send({ type: 'START', step: 3, text: 'React Native 🚀 שלום' });
      expect(actor.getSnapshot().value).toBe('running');
      expect(actor.getSnapshot().context).toEqual({
        count: 3,
        label: 'React Native 🚀 שלום',
      });

      clock.increment(250);
      expect(actor.getSnapshot().value).toBe('finished');
      expect(actor.getSnapshot().status).toBe('done');
    } finally {
      globalThis.WebAssembly = origWasm;
      // @ts-ignore
      delete globalThis.__LAVI_FORCE_UTF8_FALLBACK__;
    }
  });

  it('supports React & React Native hooks (useMachine, useActor, useActorRef, useSelector) including StrictMode', () => {
    // @ts-ignore
    globalThis.IS_REACT_ACT_ENVIRONMENT = true;
    const container = document.createElement('div');
    document.body.appendChild(container);
    const root = createRoot(container);

    let entryCalls = 0;
    let countSelectorRenders = 0;
    const initialSnapContexts: number[] = [];
    const providedActionLog: string[] = [];

    const machine = createMachine({
      id: 'reactHookTest',
      initial: 'idle',
      context: { count: 0, unrelated: 'a', items: { first: 'alpha', second: 'beta' } },
      entry: assign({ count: ({ context }: any) => context.count + 10 }),
      states: {
        idle: {
          entry: () => {
            entryCalls += 1;
          },
          on: {
            CHILD_MOUNT: {
              actions: assign({ unrelated: () => 'mounted-by-child' }),
            },
            INC: {
              actions: [
                assign({ count: ({ context }: any) => context.count + 1 }),
                'recordProvided',
              ],
            },
            UNRELATED: {
              actions: assign({ unrelated: ({ event }: any) => event.val }),
            },
            FINISH: 'done',
          },
        },
        done: {
          type: 'final',
        },
      },
    });

    const observerMachine = createMachine({
      id: 'observerHookTest',
      initial: 'watching',
      states: { watching: {} },
    });

    let exposedSend: ((evt: any) => void) | null = null;
    let exposedActorRef: any = null;
    let setItemKeyExternal: ((key: 'first' | 'second') => void) | null = null;
    let setLabelPrefixExternal: ((prefix: string) => void) | null = null;
    const observedValues: string[] = [];

    function ChildMountSender({ actorRef }: { actorRef: any }) {
      React.useEffect(() => {
        actorRef.send({ type: 'CHILD_MOUNT' });
      }, [actorRef]);
      return null;
    }

    function CountBadge({
      actorRef,
      itemKey,
    }: {
      actorRef: any;
      itemKey: 'first' | 'second';
    }) {
      const count = useSelector(actorRef, (s: any) => s.context.count);
      const itemVal = useSelector(actorRef, (s: any) => s.context.items[itemKey]);
      const fallbackVal = useSelector(undefined, (s) => (s as any)?.context?.count ?? 42);
      countSelectorRenders += 1;
      return React.createElement(
        'div',
        null,
        React.createElement('span', { id: 'count' }, String(count)),
        React.createElement('span', { id: 'item' }, String(itemVal)),
        React.createElement('span', { id: 'fallback' }, String(fallbackVal))
      );
    }

    function TestApp() {
      const [itemKey, setItemKey] = React.useState<'first' | 'second'>('first');
      const [labelPrefix, setLabelPrefix] = React.useState('v1');
      setItemKeyExternal = setItemKey;
      setLabelPrefixExternal = setLabelPrefix;

      const [snapshot, send, actorRef] = useMachine(
        machine.provide({
          actions: {
            recordProvided: () => {
              providedActionLog.push(labelPrefix);
            },
          },
        })
      );
      if (initialSnapContexts.length === 0) {
        initialSnapContexts.push(snapshot.context.count);
      }
      const refViaHook = useActorRef(
        () => observerMachine,
        undefined,
        (snap: any) => {
          observedValues.push(snap.value);
        }
      );
      exposedSend = send;
      exposedActorRef = actorRef;

      return React.createElement(
        'div',
        null,
        React.createElement('span', { id: 'state' }, String(snapshot.value)),
        React.createElement('span', { id: 'unrelated' }, String(snapshot.context.unrelated)),
        React.createElement('span', { id: 'ref-state' }, String(refViaHook.getSnapshot().value)),
        React.createElement(ChildMountSender, { actorRef }),
        React.createElement(CountBadge, { actorRef, itemKey })
      );
    }

    act(() => {
      root.render(React.createElement(React.StrictMode, null, React.createElement(TestApp)));
    });

    // Initial functional assign entry action (0 + 10 = 10) is available on first render before effects
    expect(initialSnapContexts[0]).toBe(10);
    expect(container.querySelector('#state')?.textContent).toBe('idle');
    expect(container.querySelector('#count')?.textContent).toBe('10');
    expect(container.querySelector('#item')?.textContent).toBe('alpha');
    expect(container.querySelector('#fallback')?.textContent).toBe('42');
    // ChildMountSender's useEffect sent CHILD_MOUNT before parent useEffect and was preserved across StrictMode
    expect(container.querySelector('#unrelated')?.textContent).toBe('mounted-by-child');
    // Entry side-effect action ran exactly once despite StrictMode mount -> cleanup -> remount
    expect(entryCalls).toBe(1);
    expect(observedValues).toEqual(['watching']);

    const rendersBeforeUnrelated = countSelectorRenders;

    // Sending UNRELATED updates TestApp's snapshot, while CountBadge's selected count stays 10
    act(() => {
      exposedSend!({ type: 'UNRELATED', val: 'b' });
    });
    expect(container.querySelector('#unrelated')?.textContent).toBe('b');
    expect(container.querySelector('#count')?.textContent).toBe('10');

    // Updating a component prop used inside an inline useSelector closure re-evaluates without an actor transition
    act(() => {
      setItemKeyExternal!('second');
    });
    expect(container.querySelector('#item')?.textContent).toBe('beta');

    // Updating React state closed over by machine.provide({ actions }) invokes the latest closure
    act(() => {
      setLabelPrefixExternal!('v2');
    });

    // Sending INC updates count to 11 and invokes the latest provided action ('v2')
    act(() => {
      exposedSend!({ type: 'INC' });
    });
    expect(container.querySelector('#count')?.textContent).toBe('11');
    expect(providedActionLog).toEqual(['v2']);
    expect(countSelectorRenders).toBeGreaterThan(rendersBeforeUnrelated);

    // Sending FINISH transitions to done
    act(() => {
      exposedSend!({ type: 'FINISH' });
    });
    expect(container.querySelector('#state')?.textContent).toBe('done');
    expect(exposedActorRef.getSnapshot().status).toBe('done');

    // Also test useActor with SimulatedClock and unmount cleanup
    const clock = new SimulatedClock();
    const timerMachine = createMachine({
      id: 'timerHook',
      initial: 'waiting',
      states: {
        waiting: { after: { 100: 'ready' } },
        ready: {},
      },
    });
    let timerActorRef: any = null;
    function TimerApp() {
      const [state, , actorRef] = useActor(timerMachine, { clock });
      timerActorRef = actorRef;
      return React.createElement('div', { id: 'timer-state' }, String(state.value));
    }

    act(() => {
      root.render(React.createElement(TimerApp));
    });
    expect(container.querySelector('#timer-state')?.textContent).toBe('waiting');

    act(() => {
      clock.increment(100);
    });
    expect(container.querySelector('#timer-state')?.textContent).toBe('ready');

    act(() => {
      root.unmount();
    });
    expect(timerActorRef.getSnapshot().status).toBe('stopped');
    container.remove();
  });
});
