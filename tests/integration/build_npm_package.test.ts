import { describe, it, expect } from 'vitest';
import * as fs from 'node:fs';
import * as path from 'node:path';
import {
  createNpmPackageManifest,
  assembleNpmPackageDir,
} from '../../scripts/build-npm-package.mjs';

describe('build-npm-package (lavi-fsm)', () => {
  it('generates lavi-fsm package.json with node, bundler, web, react-native, and react conditional exports', () => {
    const manifest: any = createNpmPackageManifest('0.1.0');
    expect(manifest.name).toBe('lavi-fsm');
    expect(manifest.version).toBe('0.1.0');
    expect(manifest.main).toBe('./node/lavi.js');
    expect(manifest.module).toBe('./bundler/lavi.js');
    expect(manifest['react-native']).toBe('./react-native/lavi.js');
    expect(manifest.types).toBe('./node/lavi.d.ts');
    expect(manifest.exports['.']).toEqual({
      types: './node/lavi.d.ts',
      'react-native': './react-native/lavi.js',
      node: './node/lavi.js',
      browser: './bundler/lavi.js',
      import: './bundler/lavi.js',
      default: './node/lavi.js',
    });
    expect(manifest.exports['./react']).toEqual({
      types: './react/index.d.ts',
      'react-native': './react/index.react-native.js',
      node: './react/index.cjs',
      browser: './react/index.js',
      import: './react/index.js',
      default: './react/index.cjs',
    });
    expect(manifest.exports['./react-native']).toEqual({
      types: './react-native/lavi.d.ts',
      import: './react-native/lavi.js',
      default: './react-native/lavi.js',
    });
    expect(manifest.exports['./web']).toEqual({
      types: './web/lavi.d.ts',
      import: './web/lavi.js',
      default: './web/lavi.js',
    });
  });

  it('strips nested .gitignore and package.json files from node/, bundler/, and web/ subdirectories', () => {
    const tmpOut = path.resolve('.tmp', 'npm-dist-test');
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
