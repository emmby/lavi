import { describe, it, expect, vi, afterEach } from 'vitest';
import {
  createMachine,
  createActor,
  enqueueActions,
  raise,
  cancel,
  SimulatedClock
} from '../../lavi-wasm/pkg-npm/lavi';

afterEach(() => {
  vi.useRealTimers();
});

describe('Inspection Protocol Edge Cases & Conformance Challenger', () => {
  it('emits @xstate.event, @xstate.microstep, @xstate.action, and @xstate.snapshot for delayed after transitions and delayed raised events', () => {
    const clock = new SimulatedClock();
    const events: any[] = [];

    const machine = createMachine({
      id: 'delayedInspect',
      initial: 'waiting',
      states: {
        waiting: {
          entry: raise({ type: 'DELAYED_RAISE' }, { delay: 50, id: 'r1' }),
          on: {
            DELAYED_RAISE: {
              target: 'step2',
              actions: [{ type: 'onDelayedRaise' }]
            }
          }
        },
        step2: {
          after: {
            100: {
              target: 'done',
              actions: [{ type: 'onAfterTimeout' }]
            }
          }
        },
        done: {
          type: 'final'
        }
      }
    });

    const actor = createActor(machine, {
      clock,
      inspect: (ev: any) => {
        events.push(ev);
      }
    }).start();

    expect(actor.getSnapshot().value).toBe('waiting');
    events.length = 0;

    // 1. Advance 50ms: delayed raise fires
    clock.increment(50);
    expect(actor.getSnapshot().value).toBe('step2');

    expect(events.map((e) => e.type)).toEqual([
      '@xstate.event',
      '@xstate.action',
      '@xstate.microstep',
      '@xstate.snapshot'
    ]);
    expect(events[0].event).toEqual({ type: 'DELAYED_RAISE' });
    expect(events[1].action).toEqual({ type: 'onDelayedRaise', params: undefined });
    expect(events[2].snapshot.value).toBe('step2');
    expect(events[3].snapshot.value).toBe('step2');

    events.length = 0;

    // 2. Advance 100ms: after transition fires
    clock.increment(100);
    expect(actor.getSnapshot().value).toBe('done');
    expect(actor.getSnapshot().status).toBe('done');

    expect(events.map((e) => e.type)).toEqual([
      '@xstate.event',
      '@xstate.action',
      '@xstate.microstep',
      '@xstate.snapshot'
    ]);
    expect(events[0].event.type).toContain('xstate.after');
    expect(events[1].action).toEqual({ type: 'onAfterTimeout', params: undefined });
    expect(events[3].snapshot.value).toBe('done');
    expect(events[3].snapshot.status).toBe('done');
  });

  it('emits @xstate.action for inner non-assign actions queued inside enqueueActions while omitting the outer enqueueActions wrapper and inner assign', () => {
    const actions: any[] = [];

    const machine = createMachine(
      {
        id: 'enqueueInspect',
        initial: 'idle',
        context: { count: 0 },
        states: {
          idle: {
            on: {
              RUN: {
                target: 'active',
                actions: enqueueActions(({ enqueue }: any) => {
                  enqueue('namedAction');
                  enqueue({ type: 'paramAction', params: { level: 3 } });
                  enqueue(() => {});
                  enqueue.assign({ count: 10 });
                  enqueue.raise({ type: 'FOLLOW_UP' });
                  enqueue(cancel('timer1'));
                })
              }
            }
          },
          active: {
            on: {
              FOLLOW_UP: 'finished'
            }
          },
          finished: {}
        }
      },
      {
        actions: {
          namedAction: () => {},
          paramAction: () => {}
        }
      }
    );

    const actor = createActor(machine, {
      inspect: (ev: any) => {
        if (ev.type === '@xstate.action') {
          actions.push(ev.action);
        }
      }
    }).start();

    actor.send({ type: 'RUN' });

    expect(actor.getSnapshot().value).toBe('finished');
    expect(actor.getSnapshot().context).toEqual({ count: 10 });
    expect(actions).toEqual([
      { type: 'namedAction', params: undefined },
      { type: 'paramAction', params: { level: 3 } },
      { type: '(anonymous)', params: undefined },
      {
        type: 'xstate.raise',
        params: {
          event: { type: 'FOLLOW_UP' },
          id: undefined,
          delay: undefined
        }
      },
      {
        type: 'xstate.cancel',
        params: {
          sendId: 'timer1'
        }
      }
    ]);
  });

  it('preserves distinct action descriptors for multiple static raise and cancel actions in @xstate.action and @xstate.microstep', () => {
    const inspectedActions: any[] = [];
    const microstepActions: any[] = [];

    const machine = createMachine({
      id: 'multiRaiseCancel',
      initial: 'a',
      states: {
        a: {
          on: {
            STEP_1: {
              target: 'b',
              actions: [
                raise({ type: 'EVENT_ONE' }),
                cancel('send_alpha')
              ]
            }
          }
        },
        b: {
          on: {
            EVENT_ONE: {
              target: 'c',
              actions: [
                raise({ type: 'EVENT_TWO' }),
                cancel('send_beta')
              ]
            }
          }
        },
        c: {
          on: {
            EVENT_TWO: 'd'
          }
        },
        d: {}
      }
    });

    const actor = createActor(machine, {
      inspect: (ev: any) => {
        if (ev.type === '@xstate.action') {
          inspectedActions.push(ev.action);
        } else if (ev.type === '@xstate.microstep' && ev._transitions.length > 0) {
          for (const t of ev._transitions) {
            if (t.actions && t.actions.length > 0) {
              microstepActions.push(t.actions.map((a: any) => ({ type: a.type, params: a.params })));
            }
          }
        }
      }
    }).start();

    actor.send({ type: 'STEP_1' });
    expect(actor.getSnapshot().value).toBe('d');

    expect(inspectedActions).toEqual([
      {
        type: 'xstate.raise',
        params: { event: { type: 'EVENT_ONE' }, id: undefined, delay: undefined }
      },
      {
        type: 'xstate.cancel',
        params: { sendId: 'send_alpha' }
      },
      {
        type: 'xstate.raise',
        params: { event: { type: 'EVENT_TWO' }, id: undefined, delay: undefined }
      },
      {
        type: 'xstate.cancel',
        params: { sendId: 'send_beta' }
      }
    ]);

    expect(microstepActions).toEqual([
      [
        {
          type: 'xstate.raise',
          params: { event: { type: 'EVENT_ONE' }, id: undefined, delay: undefined }
        },
        {
          type: 'xstate.cancel',
          params: { sendId: 'send_alpha' }
        }
      ],
      [
        {
          type: 'xstate.raise',
          params: { event: { type: 'EVENT_TWO' }, id: undefined, delay: undefined }
        },
        {
          type: 'xstate.cancel',
          params: { sendId: 'send_beta' }
        }
      ]
    ]);
  });
});
