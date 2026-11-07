# Binary Distribution Archives & Release Pipeline Implementation Plan [flowos: febc8472-a87a-49dc-a90a-32a777cf3d8a]

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build multi-target binary distribution packaging and an automated tag-triggered GitHub Actions release pipeline for **npm** (`lavi-fsm`), **crates.io** (`lavi`), **PyPI** (`lavi`), and **Maven Central** (`io.github.emmby:lavi`).

**Architecture:** We add a cross-manifest version synchronizer (`scripts/sync-version.mjs`), complete registry metadata for crates.io (`lavi/Cargo.toml`) and PyPI (`lavi-py/pyproject.toml`), build a unified multi-target WASM npm package assembler (`scripts/build-npm-package.mjs` producing `dist/npm` for `lavi-fsm` with `node`, `bundler`, and `web` targets), configure Gradle `maven-publish` + `signing` + Sonatype Central Portal bundle zipping (`zipMavenCentralBundle` in `lavi-jvm/build.gradle.kts`), and wire a tag-triggered GitHub Actions workflow (`.github/workflows/release.yml`) with a setup guide (`RELEASING.md`) and GPG helper (`scripts/setup-maven-gpg.sh`).

**Tech Stack:** Rust (`cargo`), WebAssembly (`wasm-pack`), Node.js ESM (`scripts/*.mjs`), Python (`PyO3` / `maturin`), Java/Kotlin (`Gradle` `maven-publish` & `signing`), GitHub Actions, Vitest.

**Spec:** `docs/superpowers/specs/2026-11-07-binary-distribution-archives-design.md`

## Global Constraints

- **Package Identifiers**: npm = `lavi-fsm`, crates.io = `lavi`, PyPI = `lavi`, Maven Central = `io.github.emmby:lavi` (while keeping Java/Kotlin source package `io.lavi`).
- **Conformance Test Invariant**: Never modify any file in `tests/*.test.ts` (except registered meta tests if needed; place new tests in `tests/integration/`) or `tests/upstream/`. `npm run test:conformance-check` must pass at all times.
- **Local Build Path Invariant**: `npm run build:npm` (`lavi-wasm/pkg-npm`) and `npm run build:web` (`lavi-wasm/pkg`) must remain intact so existing Vitest and Playwright tests continue to work without path changes.
- **No Destructive Commands / No `/tmp`**: Never use `rm`; use `.tmp/` in the repository root for temporary test fixtures.
- **All-Four-Registries Publish Policy**: In `.github/workflows/release.yml`, when `dry_run == false`, all four registries (crates.io, npm, PyPI, Maven Central) are published strictly without silent skip-if-unconfigured fallbacks.

## Review Focus

1. **Tag Version Prefixes (`v0.1.0` vs `0.1.0`)**: `scripts/sync-version.mjs --check v0.1.0` must strip a leading `v` from git tag names before comparing against manifest versions, and reject invalid semver strings.
2. **Scoped Cargo `version` Replacement**: Updating `version = "..."` in `Cargo.toml` files must only replace the `version` key inside the `[package]` section, never clobbering dependency versions (`indexmap = { version = "2.7", ... }`).
3. **Nested `wasm-pack` Artifacts in `dist/npm`**: `wasm-pack` writes a `.gitignore` containing `*` inside every output directory (`node/`, `bundler/`, `web/`), which causes `npm pack` / `npm publish` to exclude all compiled `.wasm` and `.js` files if not removed. `scripts/build-npm-package.mjs` must strip nested `.gitignore` and `package.json` files inside subdirectories.
4. **Sonatype Central Portal Checksum & Layout Requirements**: Central Portal rejects bundle zips unless artifacts live under `io/github/emmby/lavi/<version>/` and every artifact (`.jar`, `-sources.jar`, `-javadoc.jar`, `.pom`, `.module`) has `.md5`, `.sha1`, `.sha256`, and `.sha512` companion files.
5. **Gradle Composite Build Substitution in `examples/java` & `examples/kotlin`**: Changing `lavi-jvm`'s published coordinates to `io.github.emmby:lavi` must include `dependencySubstitution` in `examples/{java,kotlin}/settings.gradle.kts` so `npm run test:examples` continues passing.

---

## Phase 1: Version Synchronization & Core Registry Metadata [flowos: e0101eae-7880-45ba-b4fa-605c9629dcf9]

### Task 1: Cross-Ecosystem Version Sync Script (`scripts/sync-version.mjs`) [x] [flowos: 5405527a-2875-4ed2-bef3-a1efacf6e0da]

**Files:**
- Create: `scripts/sync-version.mjs`
- Modify: `package.json`
- Modify: `library.json`
- Test: `tests/integration/sync_version.test.ts`

**Interfaces:**
- Consumes: Workspace manifests (`package.json`, `library.json`, `lavi/Cargo.toml`, `lavi-wasm/Cargo.toml`, `lavi-c-cpp/Cargo.toml`, `lavi-py/Cargo.toml`, `lavi-jvm/Cargo.toml`, `lavi-py/pyproject.toml`, `lavi-jvm/build.gradle.kts`, `examples/java/build.gradle.kts`, `examples/kotlin/build.gradle.kts`).
- Produces:
  - `normalizeVersion(raw: string): string` (strips optional leading `v`, validates `^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$`, throws on invalid semver)
  - `readAllManifestVersions(rootDir?: string): Record<string, string>`
  - `checkVersions(options?: { rootDir?: string, expectedVersion?: string }): { ok: boolean, version: string | null, mismatches: Array<{ file: string, actual: string, expected: string }> }`
  - `syncVersions(newVersion: string, options?: { rootDir?: string }): { version: string, updatedFiles: string[] }`

- [x] **Step 1: Write the failing test in `tests/integration/sync_version.test.ts`**

```typescript
import { describe, it, expect } from 'vitest';
import * as fs from 'node:fs';
import * as path from 'node:path';
import {
  normalizeVersion,
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

  it('updates [package] version in Cargo.toml without modifying [dependencies] versions', () => {
    const tmpRoot = path.resolve('.tmp', `sync-ver-${Date.now()}`);
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
    fs.writeFileSync(path.join(tmpRoot, 'library.json'), JSON.stringify({ name: 'lavi', version: '0.1.1' }, null, 2));
    fs.writeFileSync(path.join(tmpRoot, 'lavi-py', 'pyproject.toml'), `[project]\nname = "lavi"\nversion = "0.1.0"\n`);
    fs.writeFileSync(path.join(tmpRoot, 'lavi-jvm', 'build.gradle.kts'), `group = "io.github.emmby"\nversion = "0.1.0"\n`);
    fs.writeFileSync(path.join(tmpRoot, 'examples', 'java', 'build.gradle.kts'), `dependencies {\n    implementation("io.github.emmby:lavi:0.1.0")\n}\n`);
    fs.writeFileSync(path.join(tmpRoot, 'examples', 'kotlin', 'build.gradle.kts'), `dependencies {\n    implementation("io.github.emmby:lavi:0.1.0")\n}\n`);

    const initialCheck = checkVersions({ rootDir: tmpRoot, expectedVersion: 'v0.1.0' });
    expect(initialCheck.ok).toBe(false);
    expect(initialCheck.mismatches.map((m: any) => m.file)).toContain('library.json');

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
    expect(Object.keys(readAllManifestVersions()).length).toBe(11);
  });
});
```

- [x] **Step 2: Run test to verify it fails**

Run: `npx vitest run tests/integration/sync_version.test.ts`
Expected: FAIL with cannot find module `../../scripts/sync-version.mjs`

- [x] **Step 3: Implement `scripts/sync-version.mjs` and sync `package.json` and `library.json` to `0.1.0`**

Implement `normalizeVersion`, `readAllManifestVersions`, `checkVersions`, and `syncVersions` in `scripts/sync-version.mjs` with CLI support (`--check [expectedVersion]` and `<newVersion>`), scoped `[package]` / `[project]` TOML replacement, and run `node scripts/sync-version.mjs 0.1.0` so `package.json` (`"version": "0.1.0"`) and `library.json` (`"version": "0.1.0"`) match all other workspace manifests.

- [x] **Step 4: Run test to verify it passes**

Run: `npx vitest run tests/integration/sync_version.test.ts`
Expected: PASS (3 passed)

- [x] **Step 5: Commit**

```bash
git add scripts/sync-version.mjs package.json library.json tests/integration/sync_version.test.ts
git commit -m "feat(release): add cross-ecosystem version sync script and tests"
```

---

### Task 2: `crates.io` (`lavi`) & `PyPI` (`lavi-py`) Packaging Metadata [x] [flowos: e7ae0ae7-7073-4f9c-aa86-77c4e6840db7]

**Files:**
- Modify: `lavi/Cargo.toml`
- Create: `lavi/README.md`
- Modify: `lavi-py/pyproject.toml`
- Create: `lavi-py/README.md`
- Test: `tests/integration/registry_metadata.test.ts`

**Interfaces:**
- Consumes: `lavi/Cargo.toml`, `lavi-py/pyproject.toml`, `LICENSE`.
- Produces:
  - Publish-ready `lavi` crate manifest passing `cargo package --list --allow-dirty -p lavi`
  - Publish-ready `lavi-py/pyproject.toml` with `readme = "README.md"` and `[project.urls]` (`Homepage`, `Repository`, `Issues`)

- [x] **Step 1: Write the failing test in `tests/integration/registry_metadata.test.ts`**

```typescript
import { describe, it, expect } from 'vitest';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { spawnSync } from 'node:child_process';

describe('crates.io and PyPI registry metadata', () => {
  const rootDir = path.resolve(__dirname, '../..');

  it('includes complete crates.io metadata and README in lavi/Cargo.toml', () => {
    const cargoToml = fs.readFileSync(path.join(rootDir, 'lavi', 'Cargo.toml'), 'utf8');
    expect(cargoToml).toContain('readme = "README.md"');
    expect(cargoToml).toContain('homepage = "https://emmby.github.io/lavi/examples/"');
    expect(cargoToml).toContain('documentation = "https://docs.rs/lavi"');
    expect(cargoToml).toContain('keywords = [');
    expect(cargoToml).toContain('categories = [');
    expect(fs.existsSync(path.join(rootDir, 'lavi', 'README.md'))).toBe(true);

    const pkgList = spawnSync('cargo', ['package', '--list', '--allow-dirty', '-p', 'lavi'], {
      cwd: rootDir,
      encoding: 'utf8',
    });
    expect(pkgList.status).toBe(0);
    expect(pkgList.stdout).toContain('README.md');
    expect(pkgList.stdout).toContain('Cargo.toml');
    expect(pkgList.stdout).toContain('src/lib.rs');
  });

  it('includes README.md and [project.urls] in lavi-py/pyproject.toml', () => {
    const pyproject = fs.readFileSync(path.join(rootDir, 'lavi-py', 'pyproject.toml'), 'utf8');
    expect(pyproject).toContain('name = "lavi"');
    expect(pyproject).toContain('readme = "README.md"');
    expect(pyproject).toContain('[project.urls]');
    expect(pyproject).toContain('Repository = "https://github.com/emmby/lavi"');
    expect(fs.existsSync(path.join(rootDir, 'lavi-py', 'README.md'))).toBe(true);
  });
});
```

- [x] **Step 2: Run test to verify it fails**

Run: `npx vitest run tests/integration/registry_metadata.test.ts`
Expected: FAIL (`readme = "README.md"` missing in `lavi/Cargo.toml` and `lavi-py/README.md` missing)

- [x] **Step 3: Add metadata and READMEs to `lavi` and `lavi-py`**

Update `lavi/Cargo.toml` with `readme = "README.md"`, `homepage = "https://emmby.github.io/lavi/examples/"`, `documentation = "https://docs.rs/lavi"`, `keywords = ["statechart", "xstate", "fsm", "state-machine", "no-std"]`, `categories = ["no-std", "embedded", "simulation", "algorithms"]`, and `include = ["src/**/*", "Cargo.toml", "README.md"]`. Create `lavi/README.md` (Rust `#![no_std]` usage and overview) and `lavi-py/README.md` (Python `pip install lavi` usage and overview), and add `[project.urls]` (`Homepage`, `Repository`, `Issues`) to `lavi-py/pyproject.toml`.

- [x] **Step 4: Run test to verify it passes**

Run: `npx vitest run tests/integration/registry_metadata.test.ts`
Expected: PASS (2 passed)

- [x] **Step 5: Commit**

```bash
git add lavi/Cargo.toml lavi/README.md lavi-py/pyproject.toml lavi-py/README.md tests/integration/registry_metadata.test.ts
git commit -m "feat(packaging): add crates.io and PyPI metadata and READMEs"
```

---

## Phase 2: Unified npm & Maven Central Packaging [flowos: ba010f3c-a731-4761-b7be-7cc79e086cfc]

### Task 3: Unified `lavi-fsm` npm Package Builder (`scripts/build-npm-package.mjs`) [x] [flowos: 6a9b90c0-7f68-49f6-a1ed-8a3307554c0e]

**Files:**
- Create: `scripts/build-npm-package.mjs`
- Modify: `package.json`
- Modify: `lavi-wasm/README.md`
- Test: `tests/integration/build_npm_package.test.ts`

**Interfaces:**
- Consumes: `lavi-wasm` crate, `wasm-pack`, `lavi-wasm/README.md`, `LICENSE`.
- Produces:
  - `createNpmPackageManifest(version: string): object` (returns the `lavi-fsm` `package.json` object with `node`, `bundler`, and `web` conditional exports)
  - `assembleNpmPackageDir(options?: { rootDir?: string, outDir?: string, skipWasmBuild?: boolean }): { outDir: string, manifestPath: string, manifest: any }`
  - `npm run build:npm-dist` script in root `package.json`

- [x] **Step 1: Write the failing test in `tests/integration/build_npm_package.test.ts`**

```typescript
import { describe, it, expect } from 'vitest';
import * as fs from 'node:fs';
import * as path from 'node:path';
import {
  createNpmPackageManifest,
  assembleNpmPackageDir,
} from '../../scripts/build-npm-package.mjs';

describe('build-npm-package (lavi-fsm)', () => {
  it('generates lavi-fsm package.json with node, bundler, and web conditional exports', () => {
    const manifest: any = createNpmPackageManifest('0.1.0');
    expect(manifest.name).toBe('lavi-fsm');
    expect(manifest.version).toBe('0.1.0');
    expect(manifest.main).toBe('./node/lavi.js');
    expect(manifest.module).toBe('./bundler/lavi.js');
    expect(manifest.types).toBe('./node/lavi.d.ts');
    expect(manifest.exports['.']).toEqual({
      types: './node/lavi.d.ts',
      node: './node/lavi.js',
      browser: './bundler/lavi.js',
      import: './bundler/lavi.js',
      default: './node/lavi.js',
    });
    expect(manifest.exports['./web']).toEqual({
      types: './web/lavi.d.ts',
      import: './web/lavi.js',
      default: './web/lavi.js',
    });
  });

  it('strips nested .gitignore and package.json files from node/, bundler/, and web/ subdirectories', () => {
    const tmpOut = path.resolve('.tmp', `npm-dist-${Date.now()}`);
    for (const sub of ['node', 'bundler', 'web']) {
      const subDir = path.join(tmpOut, sub);
      fs.mkdirSync(subDir, { recursive: true });
      fs.writeFileSync(path.join(subDir, 'lavi.js'), '// stub');
      fs.writeFileSync(path.join(subDir, 'lavi_bg.wasm'), 'wasm');
      fs.writeFileSync(path.join(subDir, 'lavi.d.ts'), 'export {};');
      fs.writeFileSync(path.join(subDir, '.gitignore'), '*');
      fs.writeFileSync(path.join(subDir, 'package.json'), '{}');
    }

    const res = assembleNpmPackageDir({ outDir: tmpOut, skipWasmBuild: true });
    expect(fs.existsSync(res.manifestPath)).toBe(true);
    expect(fs.existsSync(path.join(tmpOut, 'README.md'))).toBe(true);
    expect(fs.existsSync(path.join(tmpOut, 'LICENSE'))).toBe(true);

    for (const sub of ['node', 'bundler', 'web']) {
      expect(fs.existsSync(path.join(tmpOut, sub, '.gitignore'))).toBe(false);
      expect(fs.existsSync(path.join(tmpOut, sub, 'package.json'))).toBe(false);
      expect(fs.existsSync(path.join(tmpOut, sub, 'lavi_bg.wasm'))).toBe(true);
    }
  });
});
```

- [x] **Step 2: Run test to verify it fails**

Run: `npx vitest run tests/integration/build_npm_package.test.ts`
Expected: FAIL with cannot find module `../../scripts/build-npm-package.mjs`

- [x] **Step 3: Implement `scripts/build-npm-package.mjs` and update `lavi-wasm/README.md` & `package.json`**

Implement `createNpmPackageManifest` and `assembleNpmPackageDir` in `scripts/build-npm-package.mjs` (running `wasm-pack build --target nodejs|bundler|web` into `dist/npm/{node,bundler,web}` when `skipWasmBuild` is false, removing nested `.gitignore` and `package.json` files, copying `lavi-wasm/README.md` and `LICENSE`, and writing `dist/npm/package.json`). Update `lavi-wasm/README.md` to reference `lavi-fsm` and add `"build:npm-dist": "node scripts/build-npm-package.mjs"` to root `package.json`.

- [x] **Step 4: Run test to verify it passes**

Run: `npx vitest run tests/integration/build_npm_package.test.ts`
Expected: PASS (2 passed)

- [x] **Step 5: Commit**

```bash
git add scripts/build-npm-package.mjs package.json lavi-wasm/README.md tests/integration/build_npm_package.test.ts
git commit -m "feat(wasm): add unified lavi-fsm npm package builder for node, bundler, and web"
```

---

### Task 4: Maven Central (`io.github.emmby:lavi`) Gradle Publishing & Bundle Task [x] [flowos: dfa00b54-746e-4eea-9c13-b1ccc2fa1506]

**Files:**
- Modify: `lavi-jvm/build.gradle.kts`
- Modify: `examples/java/settings.gradle.kts`
- Modify: `examples/java/build.gradle.kts`
- Modify: `examples/kotlin/settings.gradle.kts`
- Modify: `examples/kotlin/build.gradle.kts`
- Modify: `scripts/sync-version.mjs`
- Test: `tests/integration/maven_bundle.test.ts`

**Interfaces:**
- Consumes: `lavi-jvm` Gradle project and compiled JNI library.
- Produces:
  - Maven coordinates `io.github.emmby:lavi:<version>` in `lavi-jvm/build.gradle.kts` with `-sources.jar`, `-javadoc.jar`, POM metadata, and conditional PGP signing (`MAVEN_GPG_PRIVATE_KEY` & `MAVEN_GPG_PASSPHRASE`)
  - Gradle task `zipMavenCentralBundle` producing `dist/maven/lavi-maven-central-bundle.zip` with `io/github/emmby/lavi/<version>/` layout and `.md5`, `.sha1`, `.sha256`, `.sha512` checksum files

- [x] **Step 1: Write the failing test in `tests/integration/maven_bundle.test.ts`**

```typescript
import { describe, it, expect } from 'vitest';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { spawnSync } from 'node:child_process';

describe('Maven Central bundle packaging (io.github.emmby:lavi)', () => {
  const rootDir = path.resolve(__dirname, '../..');

  it('builds dist/maven/lavi-maven-central-bundle.zip with POM, sources, javadoc, and checksums', () => {
    const gradleCmd = fs.existsSync(path.join(rootDir, 'lavi-jvm', 'gradlew'))
      ? './lavi-jvm/gradlew'
      : 'gradle';
    const res = spawnSync(gradleCmd, ['-p', 'lavi-jvm', 'zipMavenCentralBundle'], {
      cwd: rootDir,
      encoding: 'utf8',
    });
    expect(res.status).toBe(0);

    const bundleZip = path.join(rootDir, 'dist', 'maven', 'lavi-maven-central-bundle.zip');
    expect(fs.existsSync(bundleZip)).toBe(true);

    const stagingDir = path.join(
      rootDir,
      'lavi-jvm',
      'build',
      'staging-deploy',
      'io',
      'github',
      'emmby',
      'lavi',
      '0.1.0'
    );
    expect(fs.existsSync(path.join(stagingDir, 'lavi-0.1.0.jar'))).toBe(true);
    expect(fs.existsSync(path.join(stagingDir, 'lavi-0.1.0-sources.jar'))).toBe(true);
    expect(fs.existsSync(path.join(stagingDir, 'lavi-0.1.0-javadoc.jar'))).toBe(true);
    expect(fs.existsSync(path.join(stagingDir, 'lavi-0.1.0.pom'))).toBe(true);

    for (const ext of ['md5', 'sha1', 'sha256', 'sha512']) {
      expect(fs.existsSync(path.join(stagingDir, `lavi-0.1.0.jar.${ext}`))).toBe(true);
      expect(fs.existsSync(path.join(stagingDir, `lavi-0.1.0.pom.${ext}`))).toBe(true);
    }
  }, 60000);
});
```

- [x] **Step 2: Run test to verify it fails**

Run: `npx vitest run tests/integration/maven_bundle.test.ts`
Expected: FAIL (`Task 'zipMavenCentralBundle' not found in root project 'lavi-jvm'`)

- [x] **Step 3: Configure `lavi-jvm/build.gradle.kts` and update `examples/{java,kotlin}` coordinates**

In `lavi-jvm/build.gradle.kts`:
- Add `maven-publish` and `signing` plugins; set `group = "io.github.emmby"`.
- Enable `withSourcesJar()` and `withJavadocJar()` in `java { ... }`.
- Configure `publishing.publications.create<MavenPublication>("mavenJava")` with `groupId = "io.github.emmby"`, `artifactId = "lavi"`, `from(components["java"])`, and full Sonatype POM metadata (`name`, `description`, `url`, MIT license, developer `emmby`, GitHub SCM).
- Configure `publishing.repositories.maven { name = "staging"; url = uri(layout.buildDirectory.dir("staging-deploy")) }`.
- Configure `signing` using `useInMemoryPgpKeys(gpgKey, gpgPassphrase)` when `System.getenv("MAVEN_GPG_PRIVATE_KEY")` is non-empty.
- Add `generateMavenChecksums` task (computing `.md5`, `.sha1`, `.sha256`, `.sha512` for all files in `build/staging-deploy` except existing checksum/signature files) and `zipMavenCentralBundle` (`Zip` task archiving `build/staging-deploy` into `../dist/maven/lavi-maven-central-bundle.zip`).
- Update `examples/java/settings.gradle.kts`, `examples/kotlin/settings.gradle.kts`, `examples/java/build.gradle.kts`, and `examples/kotlin/build.gradle.kts` to use `io.github.emmby:lavi:0.1.0` with composite-build `dependencySubstitution`.

- [x] **Step 4: Run tests to verify Maven bundle and Java/Kotlin examples pass**

Run: `npx vitest run tests/integration/maven_bundle.test.ts tests/integration/sync_version.test.ts`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add lavi-jvm/build.gradle.kts examples/java/settings.gradle.kts examples/java/build.gradle.kts examples/kotlin/settings.gradle.kts examples/kotlin/build.gradle.kts scripts/sync-version.mjs tests/integration/maven_bundle.test.ts
git commit -m "feat(jvm): configure io.github.emmby:lavi Maven Central publishing and bundle zip"
```

---

## Phase 3: GitHub Actions Release Pipeline & Setup Documentation [flowos: ea245602-1e2e-4e85-84df-5feaf06c3122]

### Task 5: GitHub Actions Release Workflow (`.github/workflows/release.yml`), GPG Script & `RELEASING.md` [x] [flowos: f0cb5744-260f-4ea2-bba2-389854aacc5f]

**Files:**
- Create: `.github/workflows/release.yml`
- Create: `scripts/setup-maven-gpg.sh`
- Create: `RELEASING.md`
- Modify: `README.md`
- Modify: `examples/js-wasm/index.mjs`
- Modify: `TECH_DEBT.md`
- Test: `tests/integration/release_workflow.test.ts`

**Interfaces:**
- Consumes: `scripts/sync-version.mjs`, `scripts/build-npm-package.mjs`, `scripts/build-release-natives.mjs`, `lavi-jvm:zipMavenCentralBundle`.
- Produces:
  - `.github/workflows/release.yml` with `verify`, `build-wasm-npm`, `build-natives-py-jvm`, and `publish` jobs
  - `scripts/setup-maven-gpg.sh` executable helper script
  - `RELEASING.md` registry setup and release runbook

- [x] **Step 1: Write the failing test in `tests/integration/release_workflow.test.ts`**

```typescript
import { describe, it, expect } from 'vitest';
import * as fs from 'node:fs';
import * as path from 'node:path';

describe('GitHub Actions release pipeline & documentation', () => {
  const rootDir = path.resolve(__dirname, '../..');

  it('defines .github/workflows/release.yml triggered on v* tags and workflow_dispatch with all 4 registries', () => {
    const wfPath = path.join(rootDir, '.github', 'workflows', 'release.yml');
    expect(fs.existsSync(wfPath)).toBe(true);
    const wf = fs.readFileSync(wfPath, 'utf8');

    expect(wf).toContain("- 'v*'");
    expect(wf).toContain('workflow_dispatch:');
    expect(wf).toContain('node scripts/sync-version.mjs --check');
    expect(wf).toContain('node scripts/build-npm-package.mjs');
    expect(wf).toContain('node scripts/build-release-natives.mjs --execute');
    expect(wf).toContain('zipMavenCentralBundle');
    expect(wf).toContain('cargo publish -p lavi');
    expect(wf).toContain('npm publish --provenance --access public');
    expect(wf).toContain('pypa/gh-action-pypi-publish@release/v1');
    expect(wf).toContain('https://central.sonatype.com/api/v1/publisher/upload');
  });

  it('provides RELEASING.md and scripts/setup-maven-gpg.sh for registry onboarding', () => {
    const releasingPath = path.join(rootDir, 'RELEASING.md');
    const gpgScriptPath = path.join(rootDir, 'scripts', 'setup-maven-gpg.sh');
    expect(fs.existsSync(releasingPath)).toBe(true);
    expect(fs.existsSync(gpgScriptPath)).toBe(true);

    const releasing = fs.readFileSync(releasingPath, 'utf8');
    expect(releasing).toContain('crates.io');
    expect(releasing).toContain('PyPI');
    expect(releasing).toContain('lavi-fsm');
    expect(releasing).toContain('io.github.emmby');
  });
});
```

- [x] **Step 2: Run test to verify it fails**

Run: `npx vitest run tests/integration/release_workflow.test.ts`
Expected: FAIL (`.github/workflows/release.yml` does not exist)

- [x] **Step 3: Create `.github/workflows/release.yml`, `scripts/setup-maven-gpg.sh`, `RELEASING.md`, and update `README.md` & `TECH_DEBT.md`**

Create `.github/workflows/release.yml` implementing the 4 jobs (`verify`, `build-wasm-npm`, `build-natives-py-jvm`, `publish`), create `scripts/setup-maven-gpg.sh` (`chmod +x`), create `RELEASING.md` with step-by-step setup instructions for crates.io, PyPI OIDC Trusted Publisher, npm (`lavi-fsm`), and Sonatype Central Portal (`io.github.emmby:lavi`), update `README.md` and `examples/js-wasm/index.mjs` comments to reference `lavi-fsm` and `io.github.emmby:lavi`, and move `TD-12` to Resolved Cleanup Items in `TECH_DEBT.md`.

- [x] **Step 4: Run test to verify it passes and run the full repository test suite**

Run: `npx vitest run tests/integration/release_workflow.test.ts && npm test`
Expected: PASS across all test suites (`test:conformance-check`, `test:native`, `test:wasm`, `test:e2e`, `test:c-cpp`, `test:py`, `test:jvm`, `test:examples`, `test:unit`).

- [x] **Step 5: Commit**

```bash
git add .github/workflows/release.yml scripts/setup-maven-gpg.sh RELEASING.md README.md examples/js-wasm/index.mjs TECH_DEBT.md tests/integration/release_workflow.test.ts
git commit -m "feat(ci): add multi-registry release workflow, GPG helper, and RELEASING.md"
```
