import { describe, it, expect } from 'vitest';
import * as fs from 'node:fs';
import * as path from 'node:path';
import {
  DESKTOP_TARGETS,
  ANDROID_TARGETS,
  buildReleasePlan,
  verifyElf16KbAlignmentOutput,
  packageAndroidAar,
  executeReleasePlan,
} from '../scripts/build-release-natives.mjs';

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

  it('enforces 16 KB page alignment link flags on 64-bit Android targets', () => {
    const plan = buildReleasePlan({ dryRun: true });
    const arm64Cmd = plan.androidCommands.find((c: any) => c.abi === 'arm64-v8a');
    expect(arm64Cmd.rustflags).toContain('-C link-arg=-Wl,-z,max-page-size=16384');
    const x86Cmd = plan.androidCommands.find((c: any) => c.abi === 'x86_64');
    expect(x86Cmd.rustflags).toContain('-C link-arg=-Wl,-z,max-page-size=16384');
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
});
