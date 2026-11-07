import { describe, it, expect, vi } from 'vitest';
import {
  createMachine,
  createActor,
  assign,
  enqueueActions,
  raise,
  cancel,
  SimulatedClock,
  and,
  or,
  not,
  stateIn
} from '../../lavi-wasm/pkg-npm/lavi';

describe('Milestone 3 Iteration 2 Challenger Suite (Action Creators & Timers)', () => {
  // =========================================================================
  // Area 1: Defect Repro 1 — Inline Predicate Functions in check()
  // =========================================================================
  describe('Area 1: Inline Predicate Functions in check()', () => {
    it('evaluates truthy and falsy inline predicate functions with dynamic context', () => {
      let truthyResult: boolean | undefined;
      let falsyResult: boolean | undefined;

      const machine = createMachine({
        context: { count: 42, active: true },
        initial: 'idle',
        states: {
          idle: {
            on: {
              TEST: {
                actions: enqueueActions(({ check }) => {
                  truthyResult = check(({ context }: any) => context.count === 42 && context.active);
                  falsyResult = check(({ context }: any) => context.count === 99 || !context.active);
                })
              }
            }
          }
        }
      });

      createActor(machine).start().send({ type: 'TEST' });

      expect(truthyResult).toBe(true);
      expect(falsyResult).toBe(false);
    });

    it('evaluates inline predicate function receiving both dynamic context and event payload', () => {
      let eventCheckResult: boolean | undefined;

      const machine = createMachine({
        context: { threshold: 10 },
        initial: 'waiting',
        states: {
          waiting: {
            on: {
              SUBMIT: {
                actions: enqueueActions(({ check }) => {
                  eventCheckResult = check(({ context, event }: any) => {
                    return event.payload > context.threshold && event.type === 'SUBMIT';
                  });
                })
              }
            }
          }
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'SUBMIT', payload: 15 });

      expect(eventCheckResult).toBe(true);
    });

    it('gates subsequent enqueued assignments based on inline predicate check results', () => {
      const machine = createMachine({
        context: { step: 0, log: [] as string[] },
        initial: 'start',
        states: {
          start: {
            on: {
              RUN: {
                actions: enqueueActions(({ enqueue, check }) => {
                  if (check(({ context }: any) => context.step === 0)) {
                    enqueue.assign({ step: 1 });
                    enqueue.assign(({ context }: any) => ({
                      log: [...context.log, 'passed_check_1']
                    }));
                  }
                  // Check updated intermediate context
                  if (check(({ context }: any) => context.step === 1)) {
                    enqueue.assign({ step: 2 });
                    enqueue.assign(({ context }: any) => ({
                      log: [...context.log, 'passed_check_2']
                    }));
                  }
                  // Falsy check should not execute
                  if (check(({ context }: any) => context.step === 99)) {
                    enqueue.assign({ step: 99 });
                    enqueue.assign(({ context }: any) => ({
                      log: [...context.log, 'should_not_run']
                    }));
                  }
                })
              }
            }
          }
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'RUN' });

      expect(actor.getSnapshot().context).toEqual({
        step: 2,
        log: ['passed_check_1', 'passed_check_2']
      });
    });
  });

  // =========================================================================
  // Area 2: Defect Repro 2 — Composite AST Guards in check()
  // =========================================================================
  describe('Area 2: Composite AST Guards in check()', () => {
    it('evaluates not(), and(), or(), and stateIn() composite guards in check()', () => {
      let notResult: boolean | undefined;
      let andResult: boolean | undefined;
      let orResult: boolean | undefined;
      let stateInResult: boolean | undefined;
      let stateInOtherResult: boolean | undefined;

      const machine = createMachine(
        {
          context: { count: 5 },
          initial: 'active',
          states: {
            active: {
              on: {
                CHECK_GUARDS: {
                  actions: enqueueActions(({ check }) => {
                    notResult = check(not('isZero'));
                    andResult = check(and(['isPositive', 'isUnderTen']));
                    orResult = check(or(['isZero', 'isPositive']));
                    stateInResult = check(stateIn('active'));
                    stateInOtherResult = check(stateIn('otherState'));
                  })
                }
              }
            },
            otherState: {}
          }
        },
        {
          guards: {
            isZero: ({ context }: any) => context.count === 0,
            isPositive: ({ context }: any) => context.count > 0,
            isUnderTen: ({ context }: any) => context.count < 10
          }
        }
      );

      createActor(machine).start().send({ type: 'CHECK_GUARDS' });

      expect(notResult).toBe(true);
      expect(andResult).toBe(true);
      expect(orResult).toBe(true);
      expect(stateInResult).toBe(true);
      expect(stateInOtherResult).toBe(false);
    });

    it('evaluates deeply nested composite AST guards and stateIn on compound states', () => {
      let complexAstResult: boolean | undefined;
      let compoundStateInResult: boolean | undefined;

      const machine = createMachine(
        {
          context: { val: 20 },
          initial: 'parent',
          states: {
            parent: {
              initial: 'childA',
              states: {
                childA: {
                  on: {
                    TEST_COMPLEX: {
                      actions: enqueueActions(({ check }) => {
                        // not(and([not('isLarge'), or(['isSmall', 'isNegative'])]))
                        // isLarge = true (20 > 15), not('isLarge') = false
                        // and([false, ...]) = false
                        // not(false) = true
                        complexAstResult = check(
                          not(
                            and([
                              not('isLarge'),
                              or(['isSmall', 'isNegative'])
                            ])
                          )
                        );

                        compoundStateInResult = check(
                          stateIn({ parent: 'childA' })
                        );
                      })
                    }
                  }
                }
              }
            }
          }
        },
        {
          guards: {
            isLarge: ({ context }: any) => context.val > 15,
            isSmall: ({ context }: any) => context.val < 5,
            isNegative: ({ context }: any) => context.val < 0
          }
        }
      );

      createActor(machine).start().send({ type: 'TEST_COMPLEX' });

      expect(complexAstResult).toBe(true);
      expect(compoundStateInResult).toBe(true);
    });

    it('evaluates parameterized guards with static and dynamic params inside composite check()', () => {
      let staticParamResult: boolean | undefined;
      let dynamicParamResult: boolean | undefined;

      const machine = createMachine(
        {
          context: { score: 75, minPass: 70 },
          initial: 'grading',
          states: {
            grading: {
              on: {
                EVALUATE: {
                  actions: enqueueActions(({ check }) => {
                    staticParamResult = check(
                      and([
                        { type: 'greaterThan', params: { threshold: 50 } },
                        not({ type: 'greaterThan', params: { threshold: 100 } })
                      ])
                    );

                    dynamicParamResult = check(
                      and([
                        {
                          type: 'greaterThan',
                          params: ({ context }: any) => ({ threshold: context.minPass })
                        }
                      ])
                    );
                  })
                }
              }
            }
          }
        },
        {
          guards: {
            greaterThan: ({ context }: any, params: any) => {
              return context.score > (params?.threshold ?? 0);
            }
          }
        }
      );

      createActor(machine).start().send({ type: 'EVALUATE' });

      expect(staticParamResult).toBe(true);
      expect(dynamicParamResult).toBe(true);
    });
  });

  // =========================================================================
  // Area 3: Defect Repro 3 — Pre-Start Snapshot Side-Effect Isolation
  // =========================================================================
  describe('Area 3: Pre-Start Snapshot Side-Effect Isolation', () => {
    it('defers entry actions inside enqueueActions during pre-start getSnapshot() while executing assigns', () => {
      let sideEffectCount = 0;
      let actionSpy = vi.fn();

      const machine = createMachine({
        context: { initialized: false, value: 0 },
        initial: 'ready',
        entry: enqueueActions(({ enqueue }) => {
          enqueue(() => {
            sideEffectCount++;
            actionSpy();
          });
          enqueue.assign({ initialized: true, value: 100 });
        }),
        states: {
          ready: {}
        }
      });

      const actor = createActor(machine);

      // Call getSnapshot() before start() multiple times
      const snapshot1 = actor.getSnapshot();
      const snapshot2 = actor.getSnapshot();

      // Invariants:
      // 1. Initial assigns must be reflected in context
      expect(snapshot1.context).toEqual({ initialized: true, value: 100 });
      expect(snapshot2.context).toEqual({ initialized: true, value: 100 });

      // 2. Side-effect closures must NOT have run
      expect(sideEffectCount).toBe(0);
      expect(actionSpy).not.toHaveBeenCalled();

      // 3. When started, side effect runs exactly once
      actor.start();
      expect(sideEffectCount).toBe(1);
      expect(actionSpy).toHaveBeenCalledTimes(1);
    });

    it('drops pre-start events and maintains side-effect isolation across multiple pre-start calls', () => {
      let customSpy = vi.fn();

      const machine = createMachine({
        context: { count: 0 },
        initial: 'idle',
        entry: enqueueActions(({ enqueue }) => {
          enqueue(customSpy);
          enqueue.assign({ count: 10 });
        }),
        states: {
          idle: {
            on: {
              INC: {
                actions: assign({ count: ({ context }: any) => context.count + 1 })
              }
            }
          }
        }
      });

      const actor = createActor(machine);

      // Pre-start event must be dropped immediately (microcontroller zero-buffering invariant)
      actor.send({ type: 'INC' });

      const snap = actor.getSnapshot();
      expect(snap.context).toEqual({ count: 10 });
      expect(customSpy).not.toHaveBeenCalled();

      // Start machine
      actor.start();
      expect(customSpy).toHaveBeenCalledTimes(1);

      // Post-start event is processed
      actor.send({ type: 'INC' });
      expect(actor.getSnapshot().context).toEqual({ count: 11 });
    });
  });

  // =========================================================================
  // Area 4: Defect Repro 4 — Dynamic raise with Named Delays
  // =========================================================================
  describe('Area 4: Dynamic raise with Named Delays', () => {
    it('resolves named delay when raise event is a dynamic expression', () => {
      const clock = new SimulatedClock();
      const events: string[] = [];

      const machine = createMachine(
        {
          initial: 'start',
          states: {
            start: {
              on: {
                TRIGGER: {
                  actions: raise(
                    ({ context }: any) => ({ type: 'DYNAMIC_FIRED', from: 'dynamic' }),
                    { delay: 'customDelay' }
                  )
                },
                DYNAMIC_FIRED: {
                  actions: () => events.push('DYNAMIC_FIRED')
                }
              }
            }
          }
        },
        {
          delays: {
            customDelay: 100
          }
        }
      );

      const actor = createActor(machine, { clock }).start();
      actor.send({ type: 'TRIGGER' });

      // At t=0: must NOT fire prematurely
      expect(events).toEqual([]);

      // At t=50: still must not fire
      clock.set(50);
      expect(events).toEqual([]);

      // At t=100: timer reaches deadline and fires
      clock.set(100);
      expect(events).toEqual(['DYNAMIC_FIRED']);
    });

    it('resolves dynamic delay function expression when raising dynamic event', () => {
      const clock = new SimulatedClock();
      const recorded: any[] = [];

      const machine = createMachine({
        context: { baseDelay: 120, tag: 'exp1' },
        initial: 'idle',
        states: {
          idle: {
            on: {
              GO: {
                actions: raise(
                  ({ context }: any) => ({ type: 'COMPLETED', tag: context.tag }),
                  { delay: ({ context }: any) => context.baseDelay }
                )
              },
              COMPLETED: {
                actions: ({ event }: any) => recorded.push(event.tag)
              }
            }
          }
        }
      });

      const actor = createActor(machine, { clock }).start();
      actor.send({ type: 'GO' });

      clock.set(60);
      expect(recorded).toEqual([]);

      clock.set(119);
      expect(recorded).toEqual([]);

      clock.set(120);
      expect(recorded).toEqual(['exp1']);
    });
  });

  // =========================================================================
  // Area 5: Defect Repro 5 — Delayed raise in enqueueActions
  // =========================================================================
  describe('Area 5: Delayed raise in enqueueActions', () => {
    it('schedules delayed raise into effects.delayed_events from enqueueActions', () => {
      const clock = new SimulatedClock();
      const received: string[] = [];

      const machine = createMachine(
        {
          context: { extra: 'payload' },
          initial: 'idle',
          states: {
            idle: {
              on: {
                START_ENQUEUED: {
                  actions: enqueueActions(({ enqueue }) => {
                    // Literal numeric delay
                    enqueue(raise({ type: 'DELAYED_NUM' }, { delay: 100 }));
                    // Named delay string
                    enqueue(raise({ type: 'DELAYED_NAMED' }, { delay: 'stepDelay' }));
                    // Dynamic expression with delay
                    enqueue(
                      raise(
                        ({ context }: any) => ({ type: 'DELAYED_EXPR', extra: context.extra }),
                        { delay: 200 }
                      )
                    );
                  })
                },
                DELAYED_NUM: {
                  actions: () => received.push('DELAYED_NUM')
                },
                DELAYED_NAMED: {
                  actions: () => received.push('DELAYED_NAMED')
                },
                DELAYED_EXPR: {
                  actions: () => received.push('DELAYED_EXPR')
                }
              }
            }
          }
        },
        {
          delays: {
            stepDelay: 150
          }
        }
      );

      const actor = createActor(machine, { clock }).start();
      actor.send({ type: 'START_ENQUEUED' });

      // At t=0: none fired
      expect(received).toEqual([]);

      // At t=100: DELAYED_NUM fires
      clock.set(100);
      expect(received).toEqual(['DELAYED_NUM']);

      // At t=150: DELAYED_NAMED fires
      clock.set(150);
      expect(received).toEqual(['DELAYED_NUM', 'DELAYED_NAMED']);

      // At t=200: DELAYED_EXPR fires
      clock.set(200);
      expect(received).toEqual(['DELAYED_NUM', 'DELAYED_NAMED', 'DELAYED_EXPR']);
    });

    it('cancels pending delayed raises enqueued inside enqueueActions using cancel()', () => {
      const clock = new SimulatedClock();
      const events: string[] = [];

      const machine = createMachine({
        initial: 'step1',
        states: {
          step1: {
            on: {
              SCHEDULE: {
                actions: enqueueActions(({ enqueue }) => {
                  enqueue(raise({ type: 'SHOULD_CANCEL' }, { delay: 100, id: 'timer1' }));
                  enqueue(raise({ type: 'SHOULD_SURVIVE' }, { delay: 150, id: 'timer2' }));
                })
              },
              CANCEL_TIMER: {
                actions: enqueueActions(({ enqueue }) => {
                  enqueue(cancel('timer1'));
                })
              },
              SHOULD_CANCEL: {
                actions: () => events.push('CANCELLED')
              },
              SHOULD_SURVIVE: {
                actions: () => events.push('SURVIVED')
              }
            }
          }
        }
      });

      const actor = createActor(machine, { clock }).start();
      actor.send({ type: 'SCHEDULE' });

      // Advance to 50ms and cancel timer1
      clock.set(50);
      actor.send({ type: 'CANCEL_TIMER' });

      // Advance past timer1 (100ms) and timer2 (150ms)
      clock.set(160);

      // Only timer2 should have survived and fired
      expect(events).toEqual(['SURVIVED']);
    });
  });

  // =========================================================================
  // Area 6: Combinatorial & Adversarial Stress Tests
  // =========================================================================
  describe('Area 6: Combinatorial & Adversarial Stress Tests', () => {
    it('exercises complex interweaving of assign, check, immediate raise, and delayed raise', () => {
      const clock = new SimulatedClock();
      const auditLog: string[] = [];

      const machine = createMachine(
        {
          context: { phase: 0, points: 10 },
          initial: 'phaseA',
          states: {
            phaseA: {
              on: {
                START: {
                  actions: enqueueActions(({ enqueue, check }) => {
                    auditLog.push('start_action');
                    enqueue.assign({ points: 20 });

                    // Intermediate check sees updated points
                    const isTwenty = check(({ context }: any) => context.points === 20);
                    expect(isTwenty).toBe(true);

                    if (check(and(['isScoreAbove15', stateIn('phaseA')]))) {
                      // Immediate microstep raise
                      enqueue.raise({ type: 'STEP_READY' });
                      // Delayed raise
                      enqueue(raise({ type: 'TIMEOUT_FALLBACK' }, { delay: 'fallbackDelay' }));
                    }

                    enqueue.assign({ phase: 1 });
                  })
                },
                STEP_READY: {
                  target: 'phaseB',
                  actions: () => auditLog.push('transition_to_phaseB')
                },
                TIMEOUT_FALLBACK: {
                  actions: () => auditLog.push('fallback_triggered')
                }
              }
            },
            phaseB: {
              entry: () => auditLog.push('enter_phaseB')
            }
          }
        },
        {
          guards: {
            isScoreAbove15: ({ context }: any) => context.points > 15
          },
          delays: {
            fallbackDelay: 300
          }
        }
      );

      const actor = createActor(machine, { clock }).start();
      actor.send({ type: 'START' });

      // Microstep event STEP_READY should have run immediately, transitioning to phaseB
      expect(actor.getSnapshot().value).toBe('phaseB');
      expect(actor.getSnapshot().context).toEqual({ phase: 1, points: 20 });
      expect(auditLog).toEqual([
        'start_action',
        'transition_to_phaseB',
        'enter_phaseB'
      ]);

      // Advance clock past fallback delay
      clock.set(350);
      // TIMEOUT_FALLBACK was scheduled in phaseA, so in phaseB it does not trigger phaseA transition
      expect(auditLog).toEqual([
        'start_action',
        'transition_to_phaseB',
        'enter_phaseB'
      ]);
    });

    it('handles falsy context edge cases (0, false, empty string, null) in guards and assignments', () => {
      const machine = createMachine({
        context: { num: 0, flag: false, str: '', nil: null as any },
        initial: 'checkFalsy',
        states: {
          checkFalsy: {
            on: {
              RUN_CHECKS: {
                actions: enqueueActions(({ enqueue, check }) => {
                  expect(check(({ context }: any) => context.num === 0)).toBe(true);
                  expect(check(({ context }: any) => context.flag === false)).toBe(true);
                  expect(check(({ context }: any) => context.str === '')).toBe(true);
                  expect(check(({ context }: any) => context.nil === null)).toBe(true);

                  // Mutate to other falsy / truthy values
                  enqueue.assign({ num: -1, flag: true, str: 'ok', nil: { active: true } });

                  expect(check(({ context }: any) => context.num === -1)).toBe(true);
                  expect(check(({ context }: any) => context.flag === true)).toBe(true);
                  expect(check(({ context }: any) => context.str === 'ok')).toBe(true);
                  expect(check(({ context }: any) => context.nil?.active === true)).toBe(true);
                })
              }
            }
          }
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'RUN_CHECKS' });

      expect(actor.getSnapshot().context).toEqual({
        num: -1,
        flag: true,
        str: 'ok',
        nil: { active: true }
      });
    });

    it('evaluates check() gracefully with invalid inputs and depth boundary', () => {
      let results: any = {};
      const machine = createMachine({
        initial: 'testBounds',
        states: {
          testBounds: {
            on: {
              RUN_BOUNDS: {
                actions: enqueueActions(({ check }) => {
                  results.nullCheck = check(null as any);
                  results.undefinedCheck = check(undefined as any);
                  results.numberCheck = check(12345 as any);
                  results.emptyObjCheck = check({} as any);
                  results.missingGuard = check('nonExistentGuard');
                  results.emptyAnd = check(and([]));
                  results.emptyOr = check(or([]));

                  // Construct AST with depth > 50 for and()
                  let deepAnd: any = 'isAlwaysTrue';
                  for (let i = 0; i < 55; i++) {
                    deepAnd = and([deepAnd]);
                  }
                  results.deepAnd = check(deepAnd);

                  // Shallow not chains within depth bound:
                  // 10 negations of true -> true
                  let not10: any = 'isAlwaysTrue';
                  for (let i = 0; i < 10; i++) {
                    not10 = not(not10);
                  }
                  results.not10 = check(not10);

                  // 11 negations of true -> false
                  let not11: any = 'isAlwaysTrue';
                  for (let i = 0; i < 11; i++) {
                    not11 = not(not11);
                  }
                  results.not11 = check(not11);

                  // For any not chain exceeding max depth (>= 51), recursion halts at depth 51,
                  // returning false, and unwinding exactly 51 negations: 51 negations of false = true.
                  let deepNot: any = 'isAlwaysTrue';
                  for (let i = 0; i < 70; i++) {
                    deepNot = not(deepNot);
                  }
                  results.deepNot = check(deepNot);
                })
              }
            }
          }
        }
      },
      {
        guards: {
          isAlwaysTrue: () => true
        }
      }
    );

      createActor(machine).start().send({ type: 'RUN_BOUNDS' });

      expect(results.nullCheck).toBe(false);
      expect(results.undefinedCheck).toBe(false);
      expect(results.numberCheck).toBe(false);
      expect(results.emptyObjCheck).toBe(false);
      expect(results.missingGuard).toBe(false);
      expect(results.emptyAnd).toBe(true);
      expect(results.emptyOr).toBe(false);
      expect(results.deepAnd).toBe(false); // Graceful recursion bounding on and
      expect(results.not10).toBe(true); // 10 negations of true = true
      expect(results.not11).toBe(false); // 11 negations of true = false
      expect(results.deepNot).toBe(true); // Truncated at depth 51: 51 negations of false = true
    });

    it('evaluates stateIn() with parallel state configurations inside check()', () => {
      let inR1A: boolean | undefined;
      let inR2B: boolean | undefined;
      let inBoth: boolean | undefined;
      let notInR1B: boolean | undefined;

      const machine = createMachine({
        type: 'parallel',
        states: {
          region1: {
            initial: 'r1_a',
            states: {
              r1_a: {},
              r1_b: {}
            }
          },
          region2: {
            initial: 'r2_b',
            states: {
              r2_a: {},
              r2_b: {}
            }
          }
        },
        on: {
          CHECK_PARALLEL: {
            actions: enqueueActions(({ check }) => {
              inR1A = check(stateIn({ region1: 'r1_a' }));
              inR2B = check(stateIn({ region2: 'r2_b' }));
              inBoth = check(
                and([
                  stateIn({ region1: 'r1_a' }),
                  stateIn({ region2: 'r2_b' })
                ])
              );
              notInR1B = check(not(stateIn({ region1: 'r1_b' })));
            })
          }
        }
      });

      createActor(machine).start().send({ type: 'CHECK_PARALLEL' });

      expect(inR1A).toBe(true);
      expect(inR2B).toBe(true);
      expect(inBoth).toBe(true);
      expect(notInR1B).toBe(true);
    });

    it('isolates side-effects and propagates context in multi-level entry enqueueActions (root + child)', () => {
      const trace: string[] = [];

      const machine = createMachine({
        context: { rootVal: 0, childVal: 0 },
        initial: 'parent',
        entry: enqueueActions(({ enqueue }) => {
          enqueue(() => trace.push('root_side_effect'));
          enqueue.assign({ rootVal: 10 });
        }),
        states: {
          parent: {
            initial: 'child',
            entry: enqueueActions(({ enqueue }) => {
              enqueue(() => trace.push('parent_side_effect'));
              enqueue.assign(({ context }: any) => ({
                childVal: context.rootVal + 5
              }));
            }),
            states: {
              child: {
                entry: enqueueActions(({ enqueue }) => {
                  enqueue(() => trace.push('child_side_effect'));
                  enqueue.assign(({ context }: any) => ({
                    childVal: context.childVal * 2
                  }));
                })
              }
            }
          }
        }
      });

      const actor = createActor(machine);

      // Pre-start inspection
      const snap = actor.getSnapshot();
      // Context should reflect: rootVal: 10 -> parent: 15 -> child: 30
      expect(snap.context).toEqual({ rootVal: 10, childVal: 30 });
      // Zero side-effects must have leaked
      expect(trace).toEqual([]);

      // Start actor
      actor.start();

      // Side-effects execute in document order: root -> parent -> child
      expect(trace).toEqual([
        'root_side_effect',
        'parent_side_effect',
        'child_side_effect'
      ]);
    });

    it('handles multiple delayed raises with identical deadlines and negative delay clamping', () => {
      const clock = new SimulatedClock();
      const order: string[] = [];

      const machine = createMachine({
        initial: 'timerTest',
        states: {
          timerTest: {
            on: {
              SCHEDULE_ALL: {
                actions: enqueueActions(({ enqueue }) => {
                  // Negative delay clamped to 0
                  enqueue(raise({ type: 'CLAMPED_ZERO' }, { delay: -50 }));
                  // Two timers with identical 80ms deadline
                  enqueue(raise({ type: 'TIMER_A' }, { delay: 80 }));
                  enqueue(raise({ type: 'TIMER_B' }, { delay: 80 }));
                })
              },
              CLAMPED_ZERO: {
                actions: () => order.push('CLAMPED_ZERO')
              },
              TIMER_A: {
                actions: () => order.push('TIMER_A')
              },
              TIMER_B: {
                actions: () => order.push('TIMER_B')
              }
            }
          }
        }
      });

      const actor = createActor(machine, { clock }).start();
      actor.send({ type: 'SCHEDULE_ALL' });

      // Clamped to 0 runs on next tick/clock advance
      clock.set(0);
      expect(order).toEqual(['CLAMPED_ZERO']);

      clock.set(79);
      expect(order).toEqual(['CLAMPED_ZERO']);

      // At 80ms, both TIMER_A and TIMER_B execute
      clock.set(80);
      expect(order).toContain('TIMER_A');
      expect(order).toContain('TIMER_B');
      expect(order.length).toBe(3);
    });
  });
});

