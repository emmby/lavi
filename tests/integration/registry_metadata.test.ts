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
    expect(pkgList.stdout).toContain('LICENSE');
    expect(pkgList.stdout).toContain('Cargo.toml');
    expect(pkgList.stdout).toContain('src/lib.rs');
  });

  it('includes README.md, LICENSE, and [project.urls] in lavi-py/pyproject.toml and generate-import-lib in lavi-py/Cargo.toml', () => {
    const pyproject = fs.readFileSync(path.join(rootDir, 'lavi-py', 'pyproject.toml'), 'utf8');
    expect(pyproject).toContain('name = "lavi"');
    expect(pyproject).toContain('readme = "README.md"');
    expect(pyproject).toContain('[project.urls]');
    expect(pyproject).toContain('Repository = "https://github.com/emmby/lavi"');
    expect(fs.existsSync(path.join(rootDir, 'lavi-py', 'README.md'))).toBe(true);
    expect(fs.existsSync(path.join(rootDir, 'lavi-py', 'LICENSE'))).toBe(true);

    const pyCargo = fs.readFileSync(path.join(rootDir, 'lavi-py', 'Cargo.toml'), 'utf8');
    expect(pyCargo).toContain('"generate-import-lib"');
  });
});
