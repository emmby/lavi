import { describe, it, expect } from 'vitest';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';
import * as crypto from 'node:crypto';
import {
  normalizeTestSource,
  checkConformanceTests
} from '../scripts/check-conformance-tests.mjs';

function sha256(content: string): string {
  return crypto.createHash('sha256').update(content, 'utf8').digest('hex');
}

describe('conformance provenance checker', () => {
  it('permits whitelisted import rewrites and explicit : any parameter annotations', () => {
    const upstream = `
import { describe, it, expect } from 'vitest';
import { createMachine, createActor, assign } from '../src/index.ts';

describe('sample', () => {
  it('updates context', () => {
    const machine = createMachine({
      context: { count: 0 },
      entry: assign(({ context }) => ({ count: context.count + 1 }))
    });
    const actor = createActor(machine).start();
    expect(actor.getSnapshot().context.count).toBe(1);
  });
});
`;
    const adapted = `
import { describe, it, expect } from 'vitest';
import { createMachine, createActor, assign } from '../portable-xstate-wasm/pkg-npm/portable_xstate';

describe('sample', () => {
  it('updates context', () => {
    const machine = createMachine({
      context: { count: 0 },
      entry: assign(({ context }: any) => ({ count: context.count + 1 }))
    });
    const actor = createActor(machine).start();
    expect(actor.getSnapshot().context.count).toBe(1);
  });
});
`;
    expect(normalizeTestSource(adapted)).toEqual(normalizeTestSource(upstream));
  });

  it('permits registered it.skip only when the test body is 100% preserved', () => {
    const upstream = `
describe('timers', () => {
  it('clears timeout on exit', () => {
    const x = 1;
    expect(x).toBe(1);
  });
});
`;
    const validSkip = `
describe('timers', () => {
  it.skip('clears timeout on exit', () => {
    const x = 1;
    expect(x).toBe(1);
  });
});
`;
    const guttedSkip = `
describe('timers', () => {
  it.skip('clears timeout on exit', () => {
    // SKIPPED
  });
});
`;
    expect(
      normalizeTestSource(validSkip, ['clears timeout on exit'])
    ).toEqual(normalizeTestSource(upstream));
    expect(
      normalizeTestSource(guttedSkip, ['clears timeout on exit'])
    ).not.toEqual(normalizeTestSource(upstream));
  });

  it('rejects unregistered it.skip, it.only, and mutated assertions or injected .tick() calls', () => {
    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'pxstate-prov-'));
    try {
      fs.mkdirSync(path.join(tmpDir, 'tests', 'upstream'), { recursive: true });
      const upstreamContent = `import { createMachine, createActor } from '../src/index.ts';
describe('m', () => {
  it('works', () => {
    const a = createActor(createMachine({ initial: 'a', states: { a: {} } })).start();
    expect(a.getSnapshot().value).toBe('a');
  });
});
`;
      fs.writeFileSync(path.join(tmpDir, 'tests', 'upstream', 'sample.test.ts'), upstreamContent);
      fs.writeFileSync(
        path.join(tmpDir, 'tests', 'upstream', 'UPSTREAM_VERSION.json'),
        JSON.stringify({
          repository: 'https://github.com/statelyai/xstate',
          tag: 'xstate@5.25.0',
          files: { 'sample.test.ts': sha256(upstreamContent) }
        })
      );
      fs.writeFileSync(
        path.join(tmpDir, 'tests', 'conformance-manifest.json'),
        JSON.stringify({
          upstreamTag: 'xstate@5.25.0',
          files: {
            'sample.test.ts': {
              status: 'strict',
              allowed_skips: []
            }
          }
        })
      );

      // 1. Injected .tick() fails
      const hackedContent = upstreamContent.replace(
        "expect(a.getSnapshot().value).toBe('a');",
        "a.tick();\n    expect(a.getSnapshot().value).toBe('a');"
      );
      fs.writeFileSync(path.join(tmpDir, 'tests', 'sample.test.ts'), hackedContent);
      const res1 = checkConformanceTests({ rootDir: tmpDir });
      expect(res1.ok).toBe(false);
      expect(res1.errors.join('\n')).toMatch(/sample\.test\.ts/);

      // 2. Unregistered it.skip fails
      const unregisteredSkip = upstreamContent.replace("it('works'", "it.skip('works'");
      fs.writeFileSync(path.join(tmpDir, 'tests', 'sample.test.ts'), unregisteredSkip);
      const res2 = checkConformanceTests({ rootDir: tmpDir });
      expect(res2.ok).toBe(false);
      expect(res2.errors.join('\n')).toMatch(/unregistered skip/i);

      // 3. Tampered upstream file fails SHA-256 check
      fs.writeFileSync(path.join(tmpDir, 'tests', 'upstream', 'sample.test.ts'), hackedContent);
      const res3 = checkConformanceTests({ rootDir: tmpDir });
      expect(res3.ok).toBe(false);
      expect(res3.errors.join('\n')).toMatch(/SHA-256 mismatch/i);
    } finally {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    }
  });

  it('passes on the repository conformance suite', () => {
    const result = checkConformanceTests();
    expect(result.errors).toEqual([]);
    expect(result.ok).toBe(true);
  });
});

describe('agent documentation & tech-debt consistency', () => {
  const rootDir = path.resolve(__dirname, '..');

  it('links every legacy-baseline conformance file in conformance-manifest.json to TECH_DEBT.md', () => {
    const manifest = JSON.parse(
      fs.readFileSync(path.join(rootDir, 'tests', 'conformance-manifest.json'), 'utf8')
    );
    const techDebt = fs.readFileSync(path.join(rootDir, 'TECH_DEBT.md'), 'utf8');
    for (const [fileName, entry] of Object.entries<any>(manifest.files)) {
      if (entry.status === 'legacy-baseline') {
        expect(entry.tech_debt_id).toMatch(/^TD-\d+$/);
        expect(techDebt).toContain(entry.tech_debt_id);
        expect(techDebt).toContain(fileName);
      }
    }
  });

  it('replaces manual STOP GATEs with automated conformance provenance checks in AGENTS.md and PROCESS.md', () => {
    const agentsMd = fs.readFileSync(path.join(rootDir, 'AGENTS.md'), 'utf8');
    const processMd = fs.readFileSync(path.join(rootDir, 'PROCESS.md'), 'utf8');

    expect(agentsMd).not.toContain('STOP GATE 1');
    expect(agentsMd).not.toContain('STOP GATE 2');
    expect(processMd).not.toContain('STOP HERE');

    expect(agentsMd).toContain('npm run test:conformance-check');
    expect(agentsMd).toContain('TECH_DEBT.md');
    expect(agentsMd).toContain('#![no_std]');
    expect(agentsMd).toContain('Do Not Copy');
    expect(processMd).toContain('npm run test:conformance-check');
    expect(processMd).toContain('TECH_DEBT.md');
  });
});

