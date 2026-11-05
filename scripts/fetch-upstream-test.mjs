import * as fs from 'node:fs';
import * as path from 'node:path';
import * as crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = path.resolve(__dirname, '..');
const DEFAULT_TAG = 'xstate@5.25.0';

function sha256(content) {
  return crypto.createHash('sha256').update(content, 'utf8').digest('hex');
}

export async function fetchUpstreamTest(
  testFileName,
  options = {}
) {
  const rootDir = options.rootDir || DEFAULT_ROOT;
  const tag = options.tag || DEFAULT_TAG;
  const encodedTag = encodeURIComponent(tag);
  const url = `https://raw.githubusercontent.com/statelyai/xstate/${encodedTag}/packages/core/test/${testFileName}`;

  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`Failed to fetch ${url}: ${response.status} ${response.statusText}`);
  }
  const content = await response.text();
  const hash = sha256(content);

  const upstreamDir = path.join(rootDir, 'tests', 'upstream');
  fs.mkdirSync(upstreamDir, { recursive: true });

  const upstreamPath = path.join(upstreamDir, testFileName);
  fs.writeFileSync(upstreamPath, content, 'utf8');

  const versionFilePath = path.join(upstreamDir, 'UPSTREAM_VERSION.json');
  let versionManifest = {
    repository: 'https://github.com/statelyai/xstate',
    tag,
    files: {}
  };
  if (fs.existsSync(versionFilePath)) {
    versionManifest = JSON.parse(fs.readFileSync(versionFilePath, 'utf8'));
    versionManifest.tag = tag;
    versionManifest.files = versionManifest.files || {};
  }
  versionManifest.files[testFileName] = hash;
  fs.writeFileSync(versionFilePath, JSON.stringify(versionManifest, null, 2) + '\n', 'utf8');

  return { upstreamPath, sha256: hash };
}

const isMain = process.argv[1] && path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url));
if (isMain) {
  const files = process.argv.slice(2);
  if (files.length === 0) {
    console.error('Usage: node scripts/fetch-upstream-test.mjs <test-file.test.ts> [...]');
    process.exit(1);
  }
  for (const file of files) {
    const { upstreamPath, sha256: hash } = await fetchUpstreamTest(file);
    console.log(`Fetched ${file} -> ${path.relative(DEFAULT_ROOT, upstreamPath)} (${hash})`);
  }
}
