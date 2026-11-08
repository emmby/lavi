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
    expect(wf).toContain('build-embedded:');
    expect(wf).toContain('espup install');
    expect(wf).toContain('node scripts/build-release-natives.mjs --execute --embedded-only');
    expect(wf).toContain('dist/embedded/lavi-embedded.zip');
    expect(wf).toContain('release-artifacts/embedded-dist/lavi-embedded.zip');
    expect(wf).toContain('zipMavenCentralBundle');
    expect(wf).toContain('cargo publish -p lavi');
    expect(wf).toContain('npm publish ./lavi-fsm-*.tgz --provenance --access public');
    expect(wf).not.toContain('rm -f');
    expect(wf).toContain('pypa/gh-action-pypi-publish@release/v1');
    expect(wf).toContain('https://central.sonatype.com/api/v1/publisher/upload');
    expect(wf).toContain('uses: ./.github/workflows/ci.yml');
    expect(wf).toContain('needs: [verify, ci]');
  });

  it('gates GitHub Pages deployment on successful CI completion and makes CI reusable', () => {
    const ciPath = path.join(rootDir, '.github', 'workflows', 'ci.yml');
    const pagesPath = path.join(rootDir, '.github', 'workflows', 'pages.yml');
    expect(fs.existsSync(ciPath)).toBe(true);
    expect(fs.existsSync(pagesPath)).toBe(true);

    const ci = fs.readFileSync(ciPath, 'utf8');
    expect(ci).toContain('workflow_call:');

    const pages = fs.readFileSync(pagesPath, 'utf8');
    expect(pages).toContain('workflow_run:');
    expect(pages).toContain('workflows: ["CI"]');
    expect(pages).toContain("github.event.workflow_run.conclusion == 'success'");
    expect(pages).toContain('github.event.workflow_run.head_sha');
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
    expect(releasing).toContain('lavi-embedded.zip');
  });
});
