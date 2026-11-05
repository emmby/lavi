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

export function trackEntries(machine: AnyStateMachine, _setupFn?: any) {
  if (seen.has(machine)) {
    throw new Error(`This helper can't accept the same machine more than once`);
  }
  seen.add(machine);

  let logs: string[] = [];

  const stack: Array<{ stateConf: any; path: string }> = [
    { stateConf: (machine as any).config, path: '__root__' }
  ];

  while (stack.length > 0) {
    const { stateConf, path } = stack.pop()!;
    if (!stateConf) continue;

    const enter = () => {
      logs.push(`enter: ${path}`);
    };
    (enter as any).__xstate_track = true;
    if (stateConf.entry === undefined) stateConf.entry = [];
    if (!Array.isArray(stateConf.entry)) stateConf.entry = [stateConf.entry];
    stateConf.entry.unshift(enter);

    const exit = () => {
      logs.push(`exit: ${path}`);
    };
    (exit as any).__xstate_track = true;
    if (stateConf.exit === undefined) stateConf.exit = [];
    if (!Array.isArray(stateConf.exit)) stateConf.exit = [stateConf.exit];
    stateConf.exit.unshift(exit);

    if (stateConf.states) {
      for (const [key, val] of Object.entries(stateConf.states)) {
        stack.push({
          stateConf: val,
          path: path === '__root__' ? key : `${path}.${key}`
        });
      }
    }
  }

  return () => {
    const flushed = logs;
    logs = [];
    return flushed;
  };
}
