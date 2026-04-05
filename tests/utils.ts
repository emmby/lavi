import { expect } from 'vitest';
import {
  AnyMachineSnapshot,
  AnyStateMachine,
  getNextSnapshot,
  matchesState,
  StateNode,
  StateValue
} from '../portable-xstate-wasm/pkg-npm/portable_xstate';

const resolveSerializedStateValue = (
  machine: AnyStateMachine,
  serialized: string
) =>
  serialized[0] === '{'
    ? machine.resolveState({ value: JSON.parse(serialized), context: {} })
    : machine.resolveState({ value: serialized, context: {} });

export function testMultiTransition(
  machine: AnyStateMachine,
  fromState: string,
  eventTypes: string
): AnyMachineSnapshot {
  const computeNext = (
    state: AnyMachineSnapshot | string,
    eventType: string
  ) => {
    if (typeof state === 'string') {
      state = resolveSerializedStateValue(machine, state);
    }
    const nextState = getNextSnapshot(machine, state, {
      type: eventType
    });
    return nextState;
  };

  const [firstEventType, ...restEvents] = eventTypes.split(/,\s?/);

  const resultState = restEvents.reduce<AnyMachineSnapshot>(
    computeNext,
    computeNext(fromState, firstEventType)
  );

  return resultState;
}

export function testAll(
  machine: AnyStateMachine,
  expected: Record<string, Record<string, StateValue | undefined>>
): void {
  Object.keys(expected).forEach((fromState) => {
    Object.keys(expected[fromState]).forEach((eventTypes) => {
      const toState = expected[fromState][eventTypes];

      it(`should go from ${fromState} to ${JSON.stringify(
        toState
      )} on ${eventTypes}`, () => {
        const resultState = testMultiTransition(machine, fromState, eventTypes);

        if (toState === undefined) {
          // undefined means that the state didn't transition
          expect(resultState.value).toEqual(
            resolveSerializedStateValue(machine, fromState).value
          );
        } else if (typeof toState === 'string') {
          expect(matchesState(toState, resultState.value)).toBeTruthy();
        } else {
          expect(resultState.value).toEqual(toState);
        }
      });
    });
  });
}

const seen = new WeakSet<AnyStateMachine>();

import * as fs from 'fs';
export function trackEntries(machine: AnyStateMachine, setupFn?: any) {
  if (seen.has(machine)) {
    throw new Error(`This helper can't accept the same machine more than once`);
  }
  seen.add(machine);

  let logs: string[] = [];

  function injectTrackers(stateConf: any, path: string) {
    if (!stateConf) return;

    if (stateConf.entry === undefined) stateConf.entry = [];
    if (!Array.isArray(stateConf.entry)) stateConf.entry = [stateConf.entry];
    stateConf.entry.unshift(() => {
      logs.push(`enter: ${path}`);
      require('fs').appendFileSync('trace.log', `ENTER ${path}\n${new Error().stack}\n\n`);
    });

    if (stateConf.exit === undefined) stateConf.exit = [];
    if (!Array.isArray(stateConf.exit)) stateConf.exit = [stateConf.exit];
    stateConf.exit.unshift(() => {
      logs.push(`exit: ${path}`);
      fs.appendFileSync('debug.log', `FIRING EXIT FOR ${path}\n${new Error().stack}\n\n`);
    });

    if (stateConf.states) {
      for (const [key, val] of Object.entries(stateConf.states)) {
        injectTrackers(val, path === '__root__' ? key : `${path}.${key}`);
      }
    }

    // Add interceptors
  }

  const config = (machine as any).config;
  if (config) {
    injectTrackers(config, '__root__');
    
    // We need setup from portable_xstate
    // However, setup is not imported here.
    const setup = setupFn || require('../portable-xstate-wasm/pkg-npm/portable_xstate.js').setup;
    const patchedMachine = setup(config).createMachine(config);
    if ((patchedMachine as any).mergeActions) {
         (patchedMachine as any).mergeActions(machine);
    }
    
    const origCreateActor = (machine as any).createActor;
    if (origCreateActor) {
      (machine as any).createActor = function(options: any) {
        console.log("HELLO XSTATE V5 createActor invoked!");
        return patchedMachine.createActor(options);
      };
      (machine as any).provide = function(impl: any, ctx: any) {
        // patchedMachine.provide returns a JS StateNode. We need to patch its createActor too.
        const provided = patchedMachine.provide(impl, ctx);
        return provided; // XState tests usually call .start on the provided machine
      };
    }
  }

  return () => {
    const flushed = logs;
    logs = [];
    return flushed;
  };
}
