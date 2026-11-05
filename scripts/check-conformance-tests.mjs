import * as fs from 'node:fs';
import * as path from 'node:path';
import * as crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = path.resolve(__dirname, '..');
const META_TEST_FILES = new Set(['conformance-check.test.ts']);

function sha256(content) {
  return crypto.createHash('sha256').update(content, 'utf8').digest('hex');
}

function escapeRegExp(str) {
  return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Normalizes a conformance test file by stripping only whitelisted mechanical adaptations:
 * 1. Top-level module import statements (and commented-out upstream imports).
 * 2. Explicit `: any` parameter type annotations required when full XState TS generics are omitted.
 * 3. `.skip` on `it.skip` / `describe.skip` / `test.skip` ONLY for test titles explicitly listed in `allowedSkips`
 *    (preserving the full test body inside the skipped block so gutted or altered bodies still fail).
 */
export function normalizeTestSource(source, allowedSkips = []) {
  let normalized = source.replace(/\r\n/g, '\n');

  // 1. Strip top-level ES module import statements (single-line and multi-line) and commented-out imports
  normalized = normalized.replace(
    /^\s*(?:\/\/\s*)?import\s+(?:type\s+)?(?:[\s\S]*?\s+from\s+)?['"][^'"]+['"]\s*;?\s*$/gm,
    ''
  );

  // 2. Strip explicit `: any` type annotations in callback parameters
  normalized = normalized.replace(/:\s*any\b(?=\s*[,)=])/g, '');

  // 3. Unwrap `.skip` ONLY on registered skip titles, keeping the test body intact
  for (const title of allowedSkips) {
    const escaped = escapeRegExp(title);
    const pattern = new RegExp(
      `\\b(it|describe|test)\\.skip(\\s*\\(\\s*(['"\`])${escaped}\\3)`,
      'g'
    );
    normalized = normalized.replace(pattern, '$1$2');
  }

  // Collapse leading/trailing blank lines and normalize multiple blank lines
  return normalized
    .split('\n')
    .map((line) => line.trimEnd())
    .join('\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}

function extractSkips(source) {
  const skips = [];
  const regex = /\b(?:it|describe|test)\.skip\s*\(\s*(['"`])((?:\\.|(?!\1).)*)\1/g;
  let match;
  while ((match = regex.exec(source)) !== null) {
    skips.push(match[2]);
  }
  return skips;
}

function findFirstDiffLine(a, b) {
  const aLines = a.split('\n');
  const bLines = b.split('\n');
  const len = Math.max(aLines.length, bLines.length);
  for (let i = 0; i < len; i++) {
    if (aLines[i] !== bLines[i]) {
      return {
        line: i + 1,
        actual: aLines[i] ?? '<EOF>',
        expected: bLines[i] ?? '<EOF>'
      };
    }
  }
  return null;
}

export function checkConformanceTests(options = {}) {
  const rootDir = options.rootDir || DEFAULT_ROOT;
  const errors = [];

  const versionPath = path.join(rootDir, 'tests', 'upstream', 'UPSTREAM_VERSION.json');
  const manifestPath = path.join(rootDir, 'tests', 'conformance-manifest.json');

  if (!fs.existsSync(versionPath)) {
    return {
      ok: false,
      errors: [`Missing upstream version manifest: ${versionPath}`]
    };
  }
  if (!fs.existsSync(manifestPath)) {
    return {
      ok: false,
      errors: [`Missing conformance manifest: ${manifestPath}`]
    };
  }

  const versionData = JSON.parse(fs.readFileSync(versionPath, 'utf8'));
  const manifestData = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));

  // 1. Verify upstream reference files against pinned SHA-256 hashes
  for (const [fileName, expectedHash] of Object.entries(versionData.files || {})) {
    const upstreamFile = path.join(rootDir, 'tests', 'upstream', fileName);
    if (!fs.existsSync(upstreamFile)) {
      errors.push(`Missing upstream reference file: tests/upstream/${fileName}`);
      continue;
    }
    const actualHash = sha256(fs.readFileSync(upstreamFile, 'utf8'));
    if (actualHash !== expectedHash) {
      errors.push(
        `SHA-256 mismatch for upstream reference tests/upstream/${fileName}: expected ${expectedHash}, got ${actualHash}. Never modify files in tests/upstream/.`
      );
    }
  }

  // 2. Discover all conformance test files in tests/*.test.ts
  const testsDir = path.join(rootDir, 'tests');
  const localTestFiles = fs
    .readdirSync(testsDir)
    .filter((f) => f.endsWith('.test.ts') && !META_TEST_FILES.has(f))
    .sort();

  const manifestFiles = manifestData.files || {};

  for (const fileName of localTestFiles) {
    if (!manifestFiles[fileName]) {
      errors.push(
        `Unregistered conformance test file tests/${fileName}: add it to tests/conformance-manifest.json and fetch its upstream reference via scripts/fetch-upstream-test.mjs.`
      );
    }
  }

  // 3. Validate each registered conformance test file
  for (const [fileName, config] of Object.entries(manifestFiles)) {
    const localPath = path.join(testsDir, fileName);
    const upstreamPath = path.join(testsDir, 'upstream', fileName);

    if (!fs.existsSync(localPath)) {
      errors.push(`Missing local conformance test file: tests/${fileName}`);
      continue;
    }
    if (!fs.existsSync(upstreamPath)) {
      errors.push(`Missing upstream reference file: tests/upstream/${fileName}`);
      continue;
    }
    if (!versionData.files || !versionData.files[fileName]) {
      errors.push(
        `Missing SHA-256 entry in tests/upstream/UPSTREAM_VERSION.json for ${fileName}. Always fetch upstream files via scripts/fetch-upstream-test.mjs.`
      );
      continue;
    }

    for (const skipEntry of config.allowed_skips || []) {
      if (!skipEntry.title || !skipEntry.category || !skipEntry.reason || !skipEntry.ref) {
        errors.push(
          `Incomplete allowed_skips entry in tests/conformance-manifest.json for ${fileName}: each skip must specify non-empty title, category, reason, and ref.`
        );
      }
    }

    const localContent = fs.readFileSync(localPath, 'utf8');
    const upstreamContent = fs.readFileSync(upstreamPath, 'utf8');

    // Forbid .only( in all conformance tests
    if (/\b(?:it|describe|test)\.only\s*\(/.test(localContent)) {
      errors.push(
        `Forbidden .only() detected in tests/${fileName}. Remove .only() before committing.`
      );
    }

    // Check legacy-baseline SHA-256 lock if applicable
    if (config.status === 'legacy-baseline') {
      const currentSha = sha256(localContent);
      if (currentSha === config.legacy_baseline_sha256) {
        continue;
      }
    }

    // Strict provenance check against upstream reference
    const allowedSkipList = (config.allowed_skips || []).map((s) => s.title);
    const allowedSkipSet = new Set(allowedSkipList);
    const actualSkips = extractSkips(localContent);

    for (const skipTitle of actualSkips) {
      if (!allowedSkipSet.has(skipTitle)) {
        errors.push(
          `Unregistered skip in tests/${fileName}: "${skipTitle}". You must not skip tests to make the suite pass unless documented in XSTATE_5_FEATURES.md and tests/conformance-manifest.json.`
        );
      }
    }

    const normalizedLocal = normalizeTestSource(localContent, allowedSkipList);
    const normalizedUpstream = normalizeTestSource(upstreamContent, []);

    if (normalizedLocal !== normalizedUpstream) {
      const diff = findFirstDiffLine(normalizedLocal, normalizedUpstream);
      const debtNote =
        config.status === 'legacy-baseline'
          ? ` (file was modified from its frozen legacy_baseline_sha256 [${config.tech_debt_id}] without matching upstream)`
          : '';
      errors.push(
        `Conformance provenance check failed for tests/${fileName}${debtNote}: normalized test content differs from tests/upstream/${fileName} at normalized line ${diff?.line ?? '?'} (got: ${JSON.stringify(diff?.actual)}, expected: ${JSON.stringify(diff?.expected)}). Never modify conformance test logic, assertions, or test bodies.`
      );
    }
  }

  return {
    ok: errors.length === 0,
    errors
  };
}

const isMain =
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url));

if (isMain) {
  const result = checkConformanceTests();
  if (!result.ok) {
    console.error('❌ Conformance test provenance check FAILED:\n');
    for (const err of result.errors) {
      console.error(`  - ${err}`);
    }
    console.error(
      '\nSee AGENTS.md and PROCESS.md for rules on upstream XState 5 conformance tests.'
    );
    process.exit(1);
  }
  console.log('✅ Conformance test provenance check passed.');
}
