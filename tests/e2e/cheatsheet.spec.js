// Tests to verify we can pass the xstate cheatsheet examples to wasm and have them work.
// The tests are currently using xstate and we are trying to migrate them to wasm.
// 
// Instructions to migrate these tests to WASM.
// - Identify the next test in cheatsheet.spec.js that is most likely to work successfully as-is using our wasm interpreter. Update that test to use the wasm interpreter instead of the xstate interpreter.
// - Identify the reason for any failures but DO NOT FIX the failures yet.
// - Without changing the test, update the WASM interpreter to match the xstate interpreter so the test passes
// - Wait for the user to instruct you to move on to the next test.

const { test, expect } = require('@playwright/test');
const { setup, createActor, assign, fromPromise, fromTransition, fromObservable, fromCallback } = require('xstate');
const fs = require('fs');
const path = require('path');

test.describe('XState Cheatsheet Examples', () => {

  // 1. Creating a state machine (Toggle)
  // Original: https://stately.ai/docs/cheatsheet#creating-a-state-machine
  test('Creating a state machine (Toggle) [WASM]', async ({ page }) => {
    const commonPath = path.resolve(__dirname, '../../examples/common.js');
    await page.goto('/examples/simple.html');
    // Import the WASM module
    // Note: In a real Node environment we might need to handle WASM loading differently (e.g. fs.readFile)
    // or rely on a bundler. Playwright's test runner supports ESM.
    const { FsmRunner, default: init } = await import('../../fsm-wasm/pkg/fsm_wasm.js');
    
    // Initialize WASM
    // In Node (non-browser), wasm-pack generated code might try to fetch/load wasm_bg.wasm.
    // We might need to pass the bytes if fetch isn't available or if path is wrong.
    const wasmPath = path.resolve(__dirname, '../../fsm-wasm/pkg/fsm_wasm_bg.wasm');
    const wasmBuffer = fs.readFileSync(wasmPath);
    const wasmModule = await WebAssembly.compile(wasmBuffer);
    await init({ module_or_path: wasmModule });
    
    const machineDef = {
      id: 'toggle',
      initial: 'active',
      context: { count: 0 },
      states: {
        active: {
          entry: assign({ count: ({ context }) => context.count + 1 }),
          on: { toggle: { target: 'inactive' } },
        },
        inactive: {
          on: { toggle: { target: 'active' } },
        },
      },
    };
    
    // const jsonStr = JSON.stringify(machineDef);
    // Pass object directly to let WASM handle inline functions (if implemented)
    const machine = new FsmRunner(machineDef);
    machine.start();
    
    // Initial state check
    // WASM API: current_state(), context_json()
    expect(machine.getSnapshot().value).toBe('active');
    // WASM: In XState, start() triggers initial entry actions. 
    // We recently fixed this parity. Count should be 1.
    expect(machine.getSnapshot().context.count).toBe(1);

    machine.send('toggle');
    expect(machine.getSnapshot().value).toBe('inactive');
    // inactive -> active (entry action increments count -> 2)
    // Wait, transition active->inactive does NOT increment. 
    // Re-entering active does.
    expect(machine.getSnapshot().context.count).toBe(1);

    machine.send('toggle');
    expect(machine.getSnapshot().value).toBe('active');
    // active entry action increments count
    expect(machine.getSnapshot().context.count).toBe(2);
  });

  // 2. Creating promise logic
  // Original: https://stately.ai/docs/cheatsheet#creating-promise-logic
  // TODO: Replace xstate with wasm
  test('Creating promise logic', async () => {
    const promiseLogic = fromPromise(async () => {
      // Mocking fetch behavior
      return { message: "https://mock.dog.ceo/image.jpg", status: "success" };
    });

    const actor = createActor(promiseLogic);
    actor.start();

    // Wait for promise to resolve
    await new Promise(res => setTimeout(res, 50));

    const snapshot = actor.getSnapshot();
    expect(snapshot.output).toEqual({
      message: "https://mock.dog.ceo/image.jpg",
      status: "success"
    });
    expect(snapshot.status).toBe('done');
  });

  // 3. Creating transition logic
  // Original: https://stately.ai/docs/cheatsheet#creating-transition-logic
  // TODO: Replace xstate with wasm
  test('Creating transition logic', async () => {
    const transitionLogic = fromTransition(
      (state, event) => {
        switch (event.type) {
          case 'inc':
            return { ...state, count: state.count + 1 };
          default:
            return state;
        }
      },
      { count: 0 } // initial state
    );

    const actor = createActor(transitionLogic);
    actor.start();

    expect(actor.getSnapshot().context).toEqual({ count: 0 });

    actor.send({ type: 'inc' });
    expect(actor.getSnapshot().context).toEqual({ count: 1 });

    actor.send({ type: 'inc' });
    expect(actor.getSnapshot().context).toEqual({ count: 2 });
  });

  // 4. Creating observable logic
  // Original: https://stately.ai/docs/cheatsheet#creating-observable-logic
  // TODO: Replace xstate with wasm
  test('Creating observable logic', async () => {
    // Mock interval observable without rxjs dependency
    const observableLogic = fromObservable(() => ({
      subscribe: (observer) => {
        let count = 0;
        const i = setInterval(() => {
          observer.next(count++);
        }, 10);
        return { unsubscribe: () => clearInterval(i) };
      }
    }));
    
    const actor = createActor(observableLogic);
    
    let lastValue = null;
    actor.subscribe((snapshot) => {
      lastValue = snapshot.context;
    });
    
    actor.start();
    
    await new Promise(res => setTimeout(res, 50));
    // Should have emitted a few values (0, 1, 2...)
    expect(lastValue).toBeGreaterThanOrEqual(0);
    
    actor.stop();
  });

  // 5. Creating callback logic
  // Original: https://stately.ai/docs/cheatsheet#creating-callback-logic
  // TODO: Replace xstate with wasm
  test('Creating callback logic', async () => {
    let receivedCancel = false;
    
    const callbackLogic = fromCallback(({ sendBack, receive }) => {
      const i = setTimeout(() => {
        sendBack({ type: 'timeout' });
      }, 100); // reduced timeout

      receive((event) => {
        if (event.type === 'cancel') {
          receivedCancel = true;
          clearTimeout(i);
        }
      });
      return () => {
        clearTimeout(i);
      };
    });

    const actor = createActor(callbackLogic);
    actor.start();
    
    actor.send({ type: 'cancel' });
    expect(receivedCancel).toBe(true);
  });

  // 6. Parent states
  // Original: https://stately.ai/docs/cheatsheet#parent-states
  // TODO: Replace xstate with wasm
  test('Parent states [WASM]', async () => {
    // Import WASM module
    const { setup: wasmSetup, createActor: wasmCreateActor, default: init } = await import('../../fsm-wasm/pkg/fsm_wasm.js');

    // Initialize WASM
    const wasmPath = path.resolve(__dirname, '../../fsm-wasm/pkg/fsm_wasm_bg.wasm');
    const wasmBuffer = fs.readFileSync(wasmPath);
    const wasmModule = await WebAssembly.compile(wasmBuffer);
    await init({ module_or_path: wasmModule });

    const machine = wasmSetup({ /* ... */}).createMachine({
      id: 'parent',
      initial: 'active',
      states: {
        active: {
          initial: 'one',
          states: {
            one: {
              on: { NEXT: { target: 'two' } },
            },
            two: {},
          },
          on: {
            NEXT: { target: 'inactive' },
          },
        },
        inactive: {},
      },
    });

    const actor = wasmCreateActor(machine);
    actor.start();

    // logs { active: 'one' }
    // WASM implementation may return string "one" or "active.one" or fail if hierarchy not implemented
    expect(actor.getSnapshot().value).toEqual({ active: 'one' });

    actor.send({ type: 'NEXT' });
    // logs { active: 'two' }
    expect(actor.getSnapshot().value).toEqual({ active: 'two' });

    actor.send({ type: 'NEXT' });
    // logs 'inactive'
    expect(actor.getSnapshot().value).toBe('inactive');
  });

  // 7. Actions
  // Original: https://stately.ai/docs/cheatsheet#actions
  // TODO: Replace xstate with wasm
  test('Actions [WASM]', async () => {
    // Import WASM module
    const { setup: wasmSetup, createActor: wasmCreateActor, default: init } = await import('../../fsm-wasm/pkg/fsm_wasm.js');

    // Initialize WASM (boilerplate)
    const fs = require('fs');
    const path = require('path');
    const wasmPath = path.resolve(__dirname, '../../fsm-wasm/pkg/fsm_wasm_bg.wasm');
    const wasmBuffer = fs.readFileSync(wasmPath);
    const wasmModule = await WebAssembly.compile(wasmBuffer);
    await init({ module_or_path: wasmModule });

    const logs = [];
    
    // Use WASM setup and createMachine
    const machine = wasmSetup({
      actions: {
        activate: () => logs.push('Activating'),
        deactivate: () => logs.push('Deactivating'),
        notify: (_, params) => logs.push(params?.message ?? 'Default message'),
      },
    }).createMachine({
      id: 'toggle',
      initial: 'active',
      states: {
        active: {
          entry: { type: 'activate' },
          exit: { type: 'deactivate' },
          on: {
            toggle: {
              target: 'inactive',
              actions: [{ type: 'notify' }],
            },
          },
        },
        inactive: {
          on: {
            toggle: {
              target: 'active',
              actions: [
                { type: 'notify', params: { message: 'Some notification' } },
              ],
            },
          },
        },
      },
    });

    const actor = wasmCreateActor(machine);
    actor.start();
    
    // Entry active -> 'Activating'
    // Entry active -> 'Activating'
    expect(logs).toContain('Activating');
    logs.length = 0; // clear/ensure empty

    actor.send({ type: 'toggle' });
    // Exit active -> 'Deactivating' (Generic action)
    // Action -> 'Default message' (Generic with params)
    expect(logs).toEqual(['Deactivating', 'Default message']);
    logs.length = 0;

    actor.send({ type: 'toggle' });
    // Action -> 'Some notification'
    // Entry active -> 'Activating'
    expect(logs).toEqual(['Some notification', 'Activating']);
  });

  // 8. Guards
  // Original: https://stately.ai/docs/cheatsheet#guards
  // TODO: Replace xstate with wasm
  test('Guards', async () => {
    const logs = [];
    const machine = setup({
      guards: {
        canBeToggled: ({ context }) => context.canActivate,
        isAfterTime: (_, params) => {
          // Mock time check: assuming we pass a boolean in params for simplicity in test
          // or we can just return true/false based on params for deterministic testing
          return params.shouldPass === true;
        },
      },
      actions: {
        notifyNotAllowed: () => {
          logs.push('Cannot be toggled');
        },
      },
    }).createMachine({
      id: 'toggle',
      initial: 'active',
      context: {
        canActivate: false,
      },
      states: {
        inactive: {
          on: {
            toggle: [
              { target: 'active', guard: 'canBeToggled' },
              { actions: 'notifyNotAllowed' },
            ],
          },
        },
        active: {
          on: {
            toggle: {
              // Guard with params
              guard: { type: 'isAfterTime', params: { shouldPass: true } },
              target: 'inactive',
            },
            failToggle: {
               guard: { type: 'isAfterTime', params: { shouldPass: false } },
               target: 'inactive',
            }
          },
        },
      },
    });

    const actor = createActor(machine);
    actor.start();

    // Active -> Inactive (guard isAfterTime is mocked to true)
    actor.send({ type: 'toggle' });
    expect(actor.getSnapshot().value).toBe('inactive');

    // Inactive -> Active (guard canBeToggled is false)
    actor.send({ type: 'toggle' });
    expect(actor.getSnapshot().value).toBe('inactive');
    expect(logs).toContain('Cannot be toggled');
  });

});
