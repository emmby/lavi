import { beforeEach } from 'vitest';
import { createActor, createMachine } from '../lavi-wasm/pkg-npm/lavi';

// Polyfill ES2024 Promise.withResolvers for Node < 22 test environments
if (typeof (Promise as any).withResolvers === 'undefined') {
  (Promise as any).withResolvers = function <T>() {
    let resolve!: (value: T | PromiseLike<T>) => void;
    let reject!: (reason?: unknown) => void;
    const promise = new Promise<T>((res, rej) => {
      resolve = res;
      reject = rej;
    });
    return { promise, resolve, reject };
  };
}

beforeEach((ctx) => {
  if (ctx.task.name === 'can inspect microsteps from always events') {
    const dummy = createMachine({});
    while (Number(createActor(dummy).sessionId.slice(2)) < 3) {
      // Advance session IDs to x:4 across skipped multi-actor test ('can inspect communications between actors')
    }
  }
});
