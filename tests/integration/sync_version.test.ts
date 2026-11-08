import { describe, it, expect } from 'vitest';
import * as fs from 'node:fs';
import * as path from 'node:path';
import {
  normalizeVersion,
  resolveExpectedVersion,
  readAllManifestVersions,
  checkVersions,
  syncVersions,
} from '../../scripts/sync-version.mjs';

describe('sync-version script', () => {
  it('normalizes v-prefixed tags and rejects invalid semver', () => {
    expect(normalizeVersion('v0.1.0')).toBe('0.1.0');
    expect(normalizeVersion('0.2.3-beta.1')).toBe('0.2.3-beta.1');
    expect(() => normalizeVersion('not-a-version')).toThrow(/Invalid semantic version/);
  });

  it('ignores GITHUB_REF_NAME when GITHUB_REF_TYPE is branch (workflow_dispatch on main)', () => {
    expect(
      resolveExpectedVersion(undefined, { GITHUB_REF_TYPE: 'branch', GITHUB_REF_NAME: 'main' })
    ).toBeUndefined();
    expect(
      resolveExpectedVersion(undefined, { GITHUB_REF_TYPE: 'tag', GITHUB_REF_NAME: 'v0.1.0' })
    ).toBe('v0.1.0');
    expect(
      resolveExpectedVersion('v0.2.0', { GITHUB_REF_TYPE: 'branch', GITHUB_REF_NAME: 'main' })
    ).toBe('v0.2.0');
  });

  it('updates [package] version in Cargo.toml without modifying [dependencies] versions', () => {
    const tmpRoot = path.resolve('.tmp', 'sync-ver-test');
    fs.mkdirSync(path.join(tmpRoot, 'lavi'), { recursive: true });
    fs.mkdirSync(path.join(tmpRoot, 'lavi-wasm'), { recursive: true });
    fs.mkdirSync(path.join(tmpRoot, 'lavi-c-cpp'), { recursive: true });
    fs.mkdirSync(path.join(tmpRoot, 'lavi-py'), { recursive: true });
    fs.mkdirSync(path.join(tmpRoot, 'lavi-jvm'), { recursive: true });
    fs.mkdirSync(path.join(tmpRoot, 'examples', 'java'), { recursive: true });
    fs.mkdirSync(path.join(tmpRoot, 'examples', 'kotlin'), { recursive: true });

    const sampleCargo = `[package]\nname = "lavi"\nversion = "0.1.0"\n\n[dependencies]\nindexmap = { version = "2.7", default-features = false }\n`;
    for (const dir of ['lavi', 'lavi-wasm', 'lavi-c-cpp', 'lavi-py', 'lavi-jvm']) {
      fs.writeFileSync(path.join(tmpRoot, dir, 'Cargo.toml'), sampleCargo);
    }
    fs.writeFileSync(path.join(tmpRoot, 'package.json'), JSON.stringify({ name: 'lavi', version: '0.1.0' }, null, 2));
    fs.writeFileSync(path.join(tmpRoot, 'lavi-c-cpp', 'library.json'), JSON.stringify({ name: 'lavi', version: '0.1.1' }, null, 2));
    fs.writeFileSync(path.join(tmpRoot, 'lavi-c-cpp', 'library.properties'), `name=lavi\nversion=0.1.1\n`);
    fs.writeFileSync(path.join(tmpRoot, 'lavi-py', 'pyproject.toml'), `[project]\nname = "lavi"\nversion = "0.1.0"\n`);
    fs.writeFileSync(path.join(tmpRoot, 'lavi-jvm', 'build.gradle.kts'), `group = "io.github.emmby"\nversion = "0.1.0"\n`);
    fs.writeFileSync(path.join(tmpRoot, 'examples', 'java', 'build.gradle.kts'), `dependencies {\n    implementation("io.github.emmby:lavi:0.1.0")\n}\n`);
    fs.writeFileSync(path.join(tmpRoot, 'examples', 'kotlin', 'build.gradle.kts'), `dependencies {\n    implementation("io.github.emmby:lavi:0.1.0")\n}\n`);

    const initialCheck = checkVersions({ rootDir: tmpRoot, expectedVersion: 'v0.1.0' });
    expect(initialCheck.ok).toBe(false);
    expect(initialCheck.mismatches.map((m: any) => m.file)).toContain('lavi-c-cpp/library.json');
    expect(initialCheck.mismatches.map((m: any) => m.file)).toContain('lavi-c-cpp/library.properties');

    syncVersions('v0.2.0', { rootDir: tmpRoot });

    const afterCheck = checkVersions({ rootDir: tmpRoot, expectedVersion: 'v0.2.0' });
    expect(afterCheck.ok).toBe(true);
    expect(afterCheck.version).toBe('0.2.0');

    const updatedCargo = fs.readFileSync(path.join(tmpRoot, 'lavi', 'Cargo.toml'), 'utf8');
    expect(updatedCargo).toContain('version = "0.2.0"');
    expect(updatedCargo).toContain('indexmap = { version = "2.7"');
  });

  it('verifies all live repository manifests are synchronized at 0.1.0', () => {
    const check = checkVersions({ expectedVersion: 'v0.1.0' });
    expect(check.mismatches).toEqual([]);
    expect(check.ok).toBe(true);
    expect(Object.keys(readAllManifestVersions()).length).toBe(12);
  });
});
