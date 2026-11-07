import { describe, it, expect } from 'vitest';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { spawnSync } from 'node:child_process';

describe('Maven Central bundle packaging (io.github.emmby:lavi)', () => {
  const rootDir = path.resolve(__dirname, '../..');

  it('builds dist/maven/lavi-maven-central-bundle.zip with POM, sources, javadoc, and checksums', () => {
    let res = spawnSync('./lavi-jvm/gradlew', ['-p', 'lavi-jvm', 'zipMavenCentralBundle'], {
      cwd: rootDir,
      encoding: 'utf8',
    });
    if (res.status !== 0) {
      res = spawnSync('gradle', ['-p', 'lavi-jvm', 'zipMavenCentralBundle'], {
        cwd: rootDir,
        encoding: 'utf8',
      });
    }
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
