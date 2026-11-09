import { describe, it, expect } from 'vitest';
import * as fs from 'node:fs';
import * as path from 'node:path';
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
import { readAllManifestVersions, checkVersions } from '../scripts/sync-version.mjs';

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
    expect(versions['lavi-dart/pubspec.yaml']).toBe('0.1.0');
    expect(versions['examples/dart/pubspec.yaml']).toBe('0.1.0');
    const check = checkVersions();
    expect(check.ok).toBe(true);
  });
});
