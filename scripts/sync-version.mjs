#!/usr/bin/env node
import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const DEFAULT_ROOT = path.resolve(__dirname, '..');

const SEMVER_REGEX = /^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/;

export const MANIFEST_SPECS = [
  { relPath: 'package.json', kind: 'json' },
  { relPath: 'lavi-c-cpp/library.json', kind: 'json' },
  { relPath: 'lavi-c-cpp/library.properties', kind: 'properties' },
  { relPath: 'lavi/Cargo.toml', kind: 'toml-section', section: 'package' },
  { relPath: 'lavi-wasm/Cargo.toml', kind: 'toml-section', section: 'package' },
  { relPath: 'lavi-c-cpp/Cargo.toml', kind: 'toml-section', section: 'package' },
  { relPath: 'lavi-py/Cargo.toml', kind: 'toml-section', section: 'package' },
  { relPath: 'lavi-jvm/Cargo.toml', kind: 'toml-section', section: 'package' },
  { relPath: 'lavi-py/pyproject.toml', kind: 'toml-section', section: 'project' },
  { relPath: 'lavi-jvm/build.gradle.kts', kind: 'gradle-version' },
  { relPath: 'examples/java/build.gradle.kts', kind: 'gradle-dep' },
  { relPath: 'examples/kotlin/build.gradle.kts', kind: 'gradle-dep' },
  { relPath: 'examples/rust/Cargo.toml', kind: 'cargo-example' },
  { relPath: 'examples/js-wasm/package.json', kind: 'npm-example' },
  { relPath: 'examples/python/pyproject.toml', kind: 'toml-section', section: 'project' },
  { relPath: 'examples/esp32/platformio.ini', kind: 'platformio-dep' },
  { relPath: 'lavi-dart/pubspec.yaml', kind: 'yaml-version' },
  { relPath: 'examples/dart/pubspec.yaml', kind: 'yaml-version' },
];

export function normalizeVersion(raw) {
  if (typeof raw !== 'string') {
    throw new Error(`Invalid semantic version: ${String(raw)}`);
  }
  const trimmed = raw.trim().replace(/^v/, '');
  if (!SEMVER_REGEX.test(trimmed)) {
    throw new Error(`Invalid semantic version: "${raw}"`);
  }
  return trimmed;
}

function readTomlSectionVersion(content, sectionName) {
  const lines = content.split(/\r?\n/);
  let inSection = false;
  for (const line of lines) {
    const secMatch = line.match(/^\s*\[([^\]]+)\]\s*$/);
    if (secMatch) {
      inSection = secMatch[1].trim() === sectionName;
      continue;
    }
    if (inSection) {
      const verMatch = line.match(/^\s*version\s*=\s*"([^"]+)"/);
      if (verMatch) {
        return verMatch[1];
      }
    }
  }
  return null;
}

function writeTomlSectionVersion(content, sectionName, newVersion) {
  const lines = content.split('\n');
  let inSection = false;
  let replaced = false;
  const out = lines.map((line) => {
    const secMatch = line.match(/^\s*\[([^\]]+)\]\s*$/);
    if (secMatch) {
      inSection = secMatch[1].trim() === sectionName;
      return line;
    }
    if (inSection && !replaced && /^\s*version\s*=\s*"[^"]+"/.test(line)) {
      replaced = true;
      return line.replace(/^(\s*version\s*=\s*)"[^"]+"/, `$1"${newVersion}"`);
    }
    return line;
  });
  return out.join('\n');
}

export function readAllManifestVersions(rootDir = DEFAULT_ROOT) {
  const result = {};
  for (const spec of MANIFEST_SPECS) {
    const fullPath = path.join(rootDir, spec.relPath);
    if (!fs.existsSync(fullPath)) continue;
    const content = fs.readFileSync(fullPath, 'utf8');
    if (spec.kind === 'json') {
      const parsed = JSON.parse(content);
      result[spec.relPath] = parsed.version ?? null;
    } else if (spec.kind === 'properties') {
      const m = content.match(/^\s*version\s*=\s*([^\r\n]+)/m);
      result[spec.relPath] = m ? m[1].trim() : null;
    } else if (spec.kind === 'toml-section') {
      result[spec.relPath] = readTomlSectionVersion(content, spec.section);
    } else if (spec.kind === 'gradle-version') {
      const m = content.match(/^\s*version\s*=\s*"([^"]+)"/m);
      result[spec.relPath] = m ? m[1] : null;
    } else if (spec.kind === 'gradle-dep') {
      const m = content.match(/implementation\("io\.(?:lavi|github\.emmby):lavi(?:-jvm)?:([^"]+)"\)/);
      result[spec.relPath] = m ? m[1] : null;
    } else if (spec.kind === 'cargo-example') {
      const pkgVer = readTomlSectionVersion(content, 'package');
      const depMatch = content.match(/^\s*lavi\s*=\s*\{[^}]*\bversion\s*=\s*"([^"]+)"/m);
      const depVer = depMatch ? depMatch[1] : null;
      result[spec.relPath] = pkgVer && pkgVer === depVer ? pkgVer : `${pkgVer ?? 'null'} (dep: ${depVer ?? 'null'})`;
    } else if (spec.kind === 'npm-example') {
      const parsed = JSON.parse(content);
      const pkgVer = parsed.version ?? null;
      const rawDep = parsed.dependencies?.['lavi-fsm'] ?? null;
      const depVer = typeof rawDep === 'string' ? rawDep.replace(/^\^/, '') : null;
      result[spec.relPath] = pkgVer && pkgVer === depVer ? pkgVer : `${pkgVer ?? 'null'} (dep: ${rawDep ?? 'null'})`;
    } else if (spec.kind === 'platformio-dep') {
      const m = content.match(/releases\/download\/v([^/\s]+)\/lavi-embedded\.zip/);
      result[spec.relPath] = m ? m[1] : null;
    } else if (spec.kind === 'yaml-version') {
      const m = content.match(/^\s*version\s*:\s*([^\r\n#]+)/m);
      result[spec.relPath] = m ? m[1].trim().replace(/^["']|["']$/g, '') : null;
    }
  }
  return result;
}

export function checkVersions(options = {}) {
  const rootDir = options.rootDir || DEFAULT_ROOT;
  const versions = readAllManifestVersions(rootDir);
  const entries = Object.entries(versions);
  const expected = options.expectedVersion
    ? normalizeVersion(options.expectedVersion)
    : entries[0]?.[1] ?? null;

  const mismatches = [];
  for (const [file, actual] of entries) {
    if (actual !== expected) {
      mismatches.push({ file, actual, expected });
    }
  }

  return {
    ok: mismatches.length === 0 && expected !== null,
    version: expected,
    mismatches,
  };
}

export function syncVersions(newVersion, options = {}) {
  const rootDir = options.rootDir || DEFAULT_ROOT;
  const version = normalizeVersion(newVersion);
  const updatedFiles = [];

  for (const spec of MANIFEST_SPECS) {
    const fullPath = path.join(rootDir, spec.relPath);
    if (!fs.existsSync(fullPath)) continue;
    const original = fs.readFileSync(fullPath, 'utf8');
    let updated = original;

    if (spec.kind === 'json') {
      const parsed = JSON.parse(original);
      const nextObj = { ...parsed };
      if (!('version' in nextObj)) {
        const entries = Object.entries(parsed);
        const nameIdx = entries.findIndex(([k]) => k === 'name');
        entries.splice(nameIdx + 1, 0, ['version', version]);
        updated = JSON.stringify(Object.fromEntries(entries), null, 2) + '\n';
      } else {
        nextObj.version = version;
        updated = JSON.stringify(nextObj, null, 2) + '\n';
      }
    } else if (spec.kind === 'properties') {
      updated = original.replace(/^(\s*version\s*=\s*)[^\r\n]+/m, `$1${version}`);
    } else if (spec.kind === 'toml-section') {
      updated = writeTomlSectionVersion(original, spec.section, version);
    } else if (spec.kind === 'gradle-version') {
      updated = original.replace(/^(\s*version\s*=\s*)"[^"]+"/m, `$1"${version}"`);
    } else if (spec.kind === 'gradle-dep') {
      updated = original.replace(
        /(implementation\("io\.(?:lavi|github\.emmby):lavi(?:-jvm)?:)[^"]+("\))/,
        `$1${version}$2`
      );
    } else if (spec.kind === 'cargo-example') {
      updated = writeTomlSectionVersion(original, 'package', version)
        .replace(/^(\s*#\s*lavi\s*=\s*)"[^"]+"/m, `$1"${version}"`)
        .replace(/^(\s*lavi\s*=\s*\{[^}]*\bversion\s*=\s*)"[^"]+"/m, `$1"${version}"`);
    } else if (spec.kind === 'npm-example') {
      const parsed = JSON.parse(original);
      parsed.version = version;
      if (parsed.dependencies && 'lavi-fsm' in parsed.dependencies) {
        parsed.dependencies['lavi-fsm'] = `^${version}`;
      }
      updated = JSON.stringify(parsed, null, 2) + '\n';
    } else if (spec.kind === 'platformio-dep') {
      updated = original.replace(
        /(releases\/download\/v)[^/\s]+(\/lavi-embedded\.zip)/g,
        `$1${version}$2`
      );
    } else if (spec.kind === 'yaml-version') {
      updated = original.replace(/^(\s*version\s*:\s*)[^\r\n#]+/m, `$1${version}`);
    }

    if (updated !== original) {
      fs.writeFileSync(fullPath, updated, 'utf8');
      updatedFiles.push(spec.relPath);
    }
  }

  return { version, updatedFiles };
}

export function resolveExpectedVersion(argVersion, env = process.env) {
  if (argVersion) return argVersion;
  if (env.GITHUB_REF_TYPE === 'tag' && env.GITHUB_REF_NAME) {
    return env.GITHUB_REF_NAME;
  }
  return undefined;
}

const isMain =
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url));

if (isMain) {
  const args = process.argv.slice(2);
  if (args[0] === '--check') {
    const expected = resolveExpectedVersion(args[1]);
    const res = checkVersions({ expectedVersion: expected });
    if (!res.ok) {
      console.error('❌ Version mismatch detected across workspace manifests:');
      for (const m of res.mismatches) {
        console.error(`  - ${m.file}: actual="${m.actual}", expected="${m.expected}"`);
      }
      process.exit(1);
    }
    console.log(`✅ All workspace manifests synchronized at version ${res.version}`);
  } else if (args[0]) {
    const res = syncVersions(args[0]);
    console.log(`✅ Synchronized version ${res.version} across ${res.updatedFiles.length} file(s).`);
  } else {
    console.error('Usage: node scripts/sync-version.mjs [--check [expectedVersion] | <newVersion>]');
    process.exit(1);
  }
}
