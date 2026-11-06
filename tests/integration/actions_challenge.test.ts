import { describe, it, expect, vi } from 'vitest';
import { createMachine, createActor, assign, raise } from '../../lavi-wasm/pkg-npm/lavi';

describe('Milestone 4 Empirical Actions Challenger Suite', () => {
  // =========================================================================
  // 1. Synchronous Reentrant Event Queuing
  // =========================================================================
  describe('Synchronous Reentrant Event Queuing', () => {
    it('handles actor.send() from within a transition action without aliasing panic', () => {
      const trace: string[] = [];

      const machine = createMachine({
        id: 'reentrantTransition',
        initial: 'step1',
        states: {
          step1: {
            on: {
              TRIGGER: {
                target: 'step2',
                actions: [
                  ({ event }) => {
                    trace.push('transition:step1->step2');
                    actor.send({ type: 'REENTRANT_FROM_ACTION' });
                  }
                ]
              }
            }
          },
          step2: {
            entry: () => {
              trace.push('entry:step2');
            },
            on: {
              REENTRANT_FROM_ACTION: {
                target: 'step3',
                actions: () => {
                  trace.push('transition:step2->step3');
                }
              }
            }
          },
          step3: {
            entry: () => {
              trace.push('entry:step3');
            }
          }
        }
      });

      const actor = createActor(machine).start();
      trace.push('started');

      actor.send({ type: 'TRIGGER' });

      // The current transition must complete first (entry:step2),
      // then the reentrant event must transition to step3
      expect(trace).toEqual([
        'started',
        'transition:step1->step2',
        'entry:step2',
        'transition:step2->step3',
        'entry:step3'
      ]);
      expect(actor.getSnapshot().matches('step3')).toBe(true);
    });

    it('handles actor.send() from within an entry action', () => {
      const trace: string[] = [];

      const machine = createMachine({
        id: 'reentrantEntry',
        initial: 'idle',
        states: {
          idle: {
            on: {
              GO: 'autoAdvancing'
            }
          },
          autoAdvancing: {
            entry: () => {
              trace.push('entry:autoAdvancing');
              actor.send({ type: 'NEXT' });
            },
            on: {
              NEXT: 'settled'
            }
          },
          settled: {
            entry: () => {
              trace.push('entry:settled');
            }
          }
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'GO' });

      expect(trace).toEqual(['entry:autoAdvancing', 'entry:settled']);
      expect(actor.getSnapshot().matches('settled')).toBe(true);
    });

    it('handles actor.send() from within an exit action', () => {
      const trace: string[] = [];

      const machine = createMachine({
        id: 'reentrantExit',
        initial: 'first',
        states: {
          first: {
            exit: () => {
              trace.push('exit:first');
              actor.send({ type: 'QUEUED_IN_EXIT' });
            },
            on: {
              LEAVE: 'second'
            }
          },
          second: {
            entry: () => {
              trace.push('entry:second');
            },
            on: {
              QUEUED_IN_EXIT: 'third'
            }
          },
          third: {
            entry: () => {
              trace.push('entry:third');
            }
          }
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'LEAVE' });

      expect(trace).toEqual([
        'exit:first',
        'entry:second',
        'entry:third'
      ]);
      expect(actor.getSnapshot().matches('third')).toBe(true);
    });

    it('processes multiple reentrant events in strict FIFO order', () => {
      const trace: string[] = [];

      const machine = createMachine({
        id: 'fifoReentrancy',
        initial: 'start',
        states: {
          start: {
            on: {
              TRIGGER: {
                target: 'collecting',
                actions: [
                  () => {
                    trace.push('queuing E1 and E2');
                    actor.send({ type: 'E1' });
                    actor.send({ type: 'E2' });
                  }
                ]
              }
            }
          },
          collecting: {
            on: {
              E1: {
                actions: () => trace.push('handled:E1')
              },
              E2: {
                target: 'done_state',
                actions: () => trace.push('handled:E2')
              }
            }
          },
          done_state: {
            type: 'final'
          }
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'TRIGGER' });

      expect(trace).toEqual([
        'queuing E1 and E2',
        'handled:E1',
        'handled:E2'
      ]);
      expect(actor.getSnapshot().status).toBe('done');
    });

    it('supports chained deep reentrant event cascading (4 levels deep)', () => {
      const levels: number[] = [];

      const machine = createMachine({
        id: 'cascadeMachine',
        initial: 'level1',
        states: {
          level1: {
            on: {
              CASCADE_1: {
                target: 'level2',
                actions: () => {
                  levels.push(1);
                  actor.send({ type: 'CASCADE_2' });
                }
              }
            }
          },
          level2: {
            on: {
              CASCADE_2: {
                target: 'level3',
                actions: () => {
                  levels.push(2);
                  actor.send({ type: 'CASCADE_3' });
                }
              }
            }
          },
          level3: {
            on: {
              CASCADE_3: {
                target: 'level4',
                actions: () => {
                  levels.push(3);
                  actor.send({ type: 'CASCADE_4' });
                }
              }
            }
          },
          level4: {
            entry: () => {
              levels.push(4);
            }
          }
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'CASCADE_1' });

      expect(levels).toEqual([1, 2, 3, 4]);
      expect(actor.getSnapshot().matches('level4')).toBe(true);
    });

    it('safely handles actor.stop() invoked from within an action callback', () => {
      const trace: string[] = [];

      const machine = createMachine({
        id: 'stopFromAction',
        initial: 'running',
        states: {
          running: {
            on: {
              HALT: {
                target: 'stopping',
                actions: [
                  () => {
                    trace.push('stopping-action');
                    actor.stop();
                    // Queue an event after stop call — should be dropped
                    actor.send({ type: 'IGNORED_POST_STOP' });
                  }
                ]
              }
            }
          },
          stopping: {
            entry: () => {
              trace.push('entry:stopping');
            },
            on: {
              IGNORED_POST_STOP: 'unreachable'
            }
          },
          unreachable: {}
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'HALT' });

      expect(trace).toEqual(['stopping-action', 'entry:stopping']);
      expect(actor.getSnapshot().status).toBe('stopped');
      expect(actor.getSnapshot().matches('unreachable')).toBe(false);
    });

    it('permits actor.getSnapshot() safely inside an action callback', () => {
      let snapshotStateValue: any = null;
      let snapshotStatus: string | null = null;

      const machine = createMachine({
        id: 'snapshotInsideAction',
        initial: 'a',
        states: {
          a: {
            on: {
              MOVE: {
                target: 'b',
                actions: () => {
                  const snap = actor.getSnapshot();
                  snapshotStateValue = snap.value;
                  snapshotStatus = snap.status;
                }
              }
            }
          },
          b: {}
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'MOVE' });

      expect(snapshotStateValue).toBeDefined();
      expect(snapshotStatus).toBe('active');
    });

    it('clears remaining reentrant events when transitioning into a final state', () => {
      const trace: string[] = [];

      const machine = createMachine({
        id: 'finalStateClearReentrant',
        initial: 'working',
        states: {
          working: {
            on: {
              FINISH: {
                target: 'done',
                actions: [
                  () => {
                    trace.push('queueing after final target');
                    actor.send({ type: 'SHOULD_BE_DROPPED' });
                  }
                ]
              }
            }
          },
          done: {
            type: 'final',
            entry: () => trace.push('entry:done')
          }
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'FINISH' });

      expect(trace).toEqual(['queueing after final target', 'entry:done']);
      expect(actor.getSnapshot().status).toBe('done');
    });
  });

  // =========================================================================
  // 2. Action Ordering: SCXML Hierarchy
  // =========================================================================
  describe('Action Ordering: SCXML Hierarchy', () => {
    it('executes actions in strict SCXML order: exit (innermost to outermost) -> transition -> entry (outermost to innermost)', () => {
      const log: string[] = [];

      const machine = createMachine({
        id: 'scxmlOrder',
        initial: 'branchA',
        states: {
          branchA: {
            initial: 'childA',
            exit: () => log.push('exit:branchA'),
            states: {
              childA: {
                initial: 'grandchildA',
                exit: () => log.push('exit:childA'),
                states: {
                  grandchildA: {
                    exit: () => log.push('exit:grandchildA')
                  }
                }
              }
            },
            on: {
              SWITCH: {
                target: 'branchB',
                actions: () => log.push('action:transition')
              }
            }
          },
          branchB: {
            initial: 'childB',
            entry: () => log.push('entry:branchB'),
            states: {
              childB: {
                initial: 'grandchildB',
                entry: () => log.push('entry:childB'),
                states: {
                  grandchildB: {
                    entry: () => log.push('entry:grandchildB')
                  }
                }
              }
            }
          }
        }
      });

      const actor = createActor(machine).start();
      log.length = 0; // Clear initial entry actions

      actor.send({ type: 'SWITCH' });

      // Expected SCXML Order:
      // 1. Innermost exited state to outermost exited state: grandchildA -> childA -> branchA
      // 2. Transition action
      // 3. Outermost entered state to innermost entered state: branchB -> childB -> grandchildB
      expect(log).toEqual([
        'exit:grandchildA',
        'exit:childA',
        'exit:branchA',
        'action:transition',
        'entry:branchB',
        'entry:childB',
        'entry:grandchildB'
      ]);
      expect(actor.getSnapshot().matches({ branchB: { childB: 'grandchildB' } })).toBe(true);
    });

    it('does not exit or re-enter common ancestor state on sibling substate transitions', () => {
      const log: string[] = [];

      const machine = createMachine({
        id: 'siblingTransition',
        initial: 'parent',
        states: {
          parent: {
            initial: 'sub1',
            entry: () => log.push('entry:parent'),
            exit: () => log.push('exit:parent'),
            states: {
              sub1: {
                exit: () => log.push('exit:sub1'),
                on: {
                  NEXT: {
                    target: 'sub2',
                    actions: () => log.push('action:transition')
                  }
                }
              },
              sub2: {
                entry: () => log.push('entry:sub2')
              }
            }
          }
        }
      });

      const actor = createActor(machine).start();
      expect(log).toEqual(['entry:parent']);
      log.length = 0;

      actor.send({ type: 'NEXT' });

      // Parent must NOT be exited or re-entered
      expect(log).toEqual([
        'exit:sub1',
        'action:transition',
        'entry:sub2'
      ]);
      expect(log).not.toContain('exit:parent');
      expect(log).not.toContain('entry:parent');
      expect(actor.getSnapshot().matches({ parent: 'sub2' })).toBe(true);
    });

    it('passes { context, event } payload faithfully to exit, transition, and entry actions', () => {
      const observed: any = {
        exitEvent: null,
        transitionEvent: null,
        entryEvent: null,
        entryContext: null
      };

      const machine = createMachine({
        id: 'payloadVerification',
        initial: 'start',
        context: { secret: 42 },
        states: {
          start: {
            exit: ({ event }) => {
              observed.exitEvent = event;
            },
            on: {
              GO: {
                target: 'dest',
                actions: ({ event }) => {
                  observed.transitionEvent = event;
                }
              }
            }
          },
          dest: {
            entry: ({ context, event }) => {
              observed.entryEvent = event;
              observed.entryContext = context;
            }
          }
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'GO', customParam: 'verified' });

      expect(observed.exitEvent).toEqual({ type: 'GO', customParam: 'verified' });
      expect(observed.transitionEvent).toEqual({ type: 'GO', customParam: 'verified' });
      expect(observed.entryEvent).toEqual({ type: 'GO', customParam: 'verified' });
      expect(observed.entryContext).toEqual({ secret: 42 });
    });
  });

  // =========================================================================
  // 3. Sequential `assign` Execution Order Mutating Context
  // =========================================================================
  describe('Sequential assign Execution Order Mutating Context', () => {
    it('mutates context sequentially so subsequent actions in the same list observe updated context', () => {
      const observations: number[] = [];

      const machine = createMachine({
        id: 'sequentialAssignMicrostep',
        initial: 'init',
        context: { count: 0 },
        states: {
          init: {
            on: {
              RUN: {
                target: 'done',
                actions: [
                  assign({ count: 1 }),
                  ({ context }) => observations.push(context.count),
                  assign({ count: 2 }),
                  ({ context }) => observations.push(context.count),
                  assign({ count: 3 }),
                  ({ context }) => observations.push(context.count)
                ]
              }
            }
          },
          done: {
            entry: ({ context }) => {
              observations.push(context.count);
            }
          }
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'RUN' });

      expect(observations).toEqual([1, 2, 3, 3]);
      expect(actor.getSnapshot().context.count).toBe(3);
    });

    it('allows functional assign to build cumulatively on preceding assign in the same microstep', () => {
      const machine = createMachine({
        id: 'functionalCumulativeAssign',
        initial: 'start',
        context: { total: 10 },
        states: {
          start: {
            on: {
              COMPUTE: {
                target: 'result',
                actions: [
                  // total = 10 + 5 = 15
                  assign({ total: ({ context }) => context.total + 5 }),
                  // total = 15 * 2 = 30
                  assign({ total: ({ context }) => context.total * 2 }),
                  // total = 30 - 3 = 27
                  assign({ total: ({ context }) => context.total - 3 })
                ]
              }
            }
          },
          result: {}
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'COMPUTE' });

      expect(actor.getSnapshot().context.total).toBe(27);
    });

    it('mutates context across exit -> transition -> entry sequentially within a single macrostep', () => {
      const machine = createMachine({
        id: 'crossStepSequentialAssign',
        initial: 'stepA',
        context: { stage: 'none', history: [] as string[] },
        states: {
          stepA: {
            exit: assign({
              stage: 'exited_A',
              history: ({ context }) => [...context.history, 'exit_A']
            }),
            on: {
              ADVANCE: {
                target: 'stepB',
                actions: assign({
                  stage: 'transitioning',
                  history: ({ context }) => [...context.history, `trans_from_${context.stage}`]
                })
              }
            }
          },
          stepB: {
            entry: assign({
              stage: 'entered_B',
              history: ({ context }) => [...context.history, `entry_from_${context.stage}`]
            })
          }
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'ADVANCE' });

      const snap = actor.getSnapshot();
      expect(snap.context.stage).toBe('entered_B');
      expect(snap.context.history).toEqual([
        'exit_A',
        'trans_from_exited_A',
        'entry_from_transitioning'
      ]);
    });
  });

  // =========================================================================
  // 4. `raise` Action Creator Execution
  // =========================================================================
  describe('raise Action Creator Execution', () => {
    it('executes immediate synchronous raise in an entry action during initialization', () => {
      const machine = createMachine({
        id: 'raiseInInitialEntry',
        initial: 'first',
        states: {
          first: {
            entry: raise({ type: 'INTERNAL_BOOT' }),
            on: {
              INTERNAL_BOOT: 'second'
            }
          },
          second: {
            entry: raise({ type: 'INTERNAL_CONFIRM' }),
            on: {
              INTERNAL_CONFIRM: 'ready'
            }
          },
          ready: {}
        }
      });

      const actor = createActor(machine).start();

      // Initialization drains internal raise events automatically
      expect(actor.getSnapshot().matches('ready')).toBe(true);
    });

    it('executes raise with payload in transition action responding immediately', () => {
      const receivedPayloads: any[] = [];

      const machine = createMachine({
        id: 'raiseWithPayload',
        initial: 'waiting',
        states: {
          waiting: {
            on: {
              START: {
                target: 'processing',
                actions: raise({ type: 'INT_EVENT', val: 999 })
              }
            }
          },
          processing: {
            on: {
              INT_EVENT: {
                target: 'complete',
                actions: ({ event }) => {
                  receivedPayloads.push(event);
                }
              }
            }
          },
          complete: {}
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'START' });

      expect(receivedPayloads).toEqual([{ type: 'INT_EVENT', val: 999 }]);
      expect(actor.getSnapshot().matches('complete')).toBe(true);
    });

    it('distinguishes delayed raise ({ delay: ... }) from immediate raise: delayed raise does not trigger immediate synchronous microstep', () => {
      const machine = createMachine({
        id: 'delayedRaiseMachine',
        initial: 'idle',
        states: {
          idle: {
            on: {
              START_TIMER: {
                target: 'counting',
                actions: raise({ type: 'TIMEOUT_RAISED' }, { delay: 100 })
              }
            }
          },
          counting: {
            on: {
              TIMEOUT_RAISED: 'alarm'
            }
          },
          alarm: {}
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'START_TIMER' });

      // Immediate state must be 'counting', NOT 'alarm' because delay: 100 was specified
      // (prevents premature synchronous execution of delayed raises)
      expect(actor.getSnapshot().matches('counting')).toBe(true);
      expect(actor.getSnapshot().matches('alarm')).toBe(false);
    });

    it('coordinates assign and raise: raised event handler observes context updated by assign', () => {
      let observedContextInRaisedHandler: any = null;

      const machine = createMachine({
        id: 'assignAndRaiseCoordination',
        initial: 'idle',
        context: { token: 'uninitialized' },
        states: {
          idle: {
            on: {
              LOGIN: {
                target: 'verifying',
                actions: [
                  assign({ token: 'xyz-secret-token' }),
                  raise({ type: 'VERIFY_TOKEN' })
                ]
              }
            }
          },
          verifying: {
            on: {
              VERIFY_TOKEN: {
                target: 'authenticated',
                actions: ({ context }) => {
                  observedContextInRaisedHandler = context.token;
                }
              }
            }
          },
          authenticated: {}
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'LOGIN' });

      expect(observedContextInRaisedHandler).toBe('xyz-secret-token');
      expect(actor.getSnapshot().matches('authenticated')).toBe(true);
      expect(actor.getSnapshot().context.token).toBe('xyz-secret-token');
    });

    it('executes raise in an exit action alongside assign', () => {
      const trace: string[] = [];

      const machine = createMachine({
        id: 'raiseInExitWithAssign',
        initial: 'first',
        context: { reason: '' },
        states: {
          first: {
            exit: [
              assign({ reason: 'departed_first' }),
              raise({ type: 'NOTIFY_EXIT' })
            ],
            on: {
              LEAVE: 'second'
            }
          },
          second: {
            on: {
              NOTIFY_EXIT: {
                target: 'third',
                actions: ({ context }) => {
                  trace.push(`handled_notify:${context.reason}`);
                }
              }
            }
          },
          third: {}
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'LEAVE' });

      expect(trace).toEqual(['handled_notify:departed_first']);
      expect(actor.getSnapshot().matches('third')).toBe(true);
    });
  });

  // =========================================================================
  // 5. Advanced Adversarial Stress & Edge Cases
  // =========================================================================
  describe('Advanced Adversarial Stress & Edge Cases', () => {
    it('drops external actor.send() called during initial state entry before machine start completes (Level 1 zero pre-start buffering invariant)', () => {
      const trace: string[] = [];

      const machine = createMachine({
        id: 'reentrantInitEntry',
        initial: 'booting',
        states: {
          booting: {
            entry: () => {
              trace.push('entry:booting');
              actor.send({ type: 'BOOT_COMPLETE' });
            },
            on: {
              BOOT_COMPLETE: 'ready'
            }
          },
          ready: {
            entry: () => {
              trace.push('entry:ready');
            }
          }
        }
      });

      const actor = createActor(machine);
      actor.start();

      // Per Level 1 Microcontroller Invariant: unstarted machines drop external events immediately
      // Initial entry action runs before machine is marked started; external send() is dropped (raise() must be used instead)
      expect(trace).toEqual(['entry:booting']);
      expect(actor.getSnapshot().matches('booting')).toBe(true);
    });

    it('ensures all parallel regions complete entry actions before any reentrant event is processed', () => {
      const order: string[] = [];

      const machine = createMachine({
        id: 'parallelReentrantBoundary',
        initial: 'init',
        states: {
          init: {
            on: {
              START_PARALLEL: 'parallel_zone'
            }
          },
          parallel_zone: {
            type: 'parallel',
            states: {
              regionA: {
                initial: 'a1',
                states: {
                  a1: {
                    entry: () => {
                      order.push('entry:regionA');
                      // Reentrant send from regionA
                      actor.send({ type: 'PARALLEL_REENTRANT' });
                    },
                    on: {
                      PARALLEL_REENTRANT: {
                        target: 'a2',
                        actions: () => order.push('action:regionA_reentrant')
                      }
                    }
                  },
                  a2: {}
                }
              },
              regionB: {
                initial: 'b1',
                states: {
                  b1: {
                    entry: () => {
                      // RegionB must finish entering before PARALLEL_REENTRANT is handled
                      order.push('entry:regionB');
                    }
                  }
                }
              }
            }
          }
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'START_PARALLEL' });

      // Both regionA and regionB must execute entry actions in the current microstep
      // BEFORE the reentrant event PARALLEL_REENTRANT is dispatched!
      expect(order).toEqual([
        'entry:regionA',
        'entry:regionB',
        'action:regionA_reentrant'
      ]);
      expect(actor.getSnapshot().matches({ parallel_zone: { regionA: 'a2', regionB: 'b1' } })).toBe(true);
    });

    it('mutates context correctly via assign inside a reentrant event transition', () => {
      const machine = createMachine({
        id: 'contextMutationInReentrant',
        initial: 'step1',
        context: { updates: [] as string[] },
        states: {
          step1: {
            on: {
              TRIGGER: {
                target: 'step2',
                actions: [
                  assign({
                    updates: ({ context }) => [...context.updates, 'from_step1']
                  }),
                  () => {
                    actor.send({ type: 'REENTRANT_ASSIGN' });
                  }
                ]
              }
            }
          },
          step2: {
            on: {
              REENTRANT_ASSIGN: {
                target: 'step3',
                actions: assign({
                  updates: ({ context }) => [...context.updates, 'from_reentrant']
                })
              }
            }
          },
          step3: {
            entry: assign({
              updates: ({ context }) => [...context.updates, 'from_step3_entry']
            })
          }
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'TRIGGER' });

      expect(actor.getSnapshot().context.updates).toEqual([
        'from_step1',
        'from_reentrant',
        'from_step3_entry'
      ]);
      expect(actor.getSnapshot().matches('step3')).toBe(true);
    });

    it('handles multi-property assign with primitive and nested object types', () => {
      const machine = createMachine({
        id: 'multiPropAssign',
        initial: 'idle',
        context: {
          str: 'old',
          num: 0,
          bool: false,
          obj: { key: 'val1' }
        },
        states: {
          idle: {
            on: {
              UPDATE: {
                actions: assign({
                  str: 'new',
                  num: 42,
                  bool: true,
                  obj: { key: 'val2', extra: 100 }
                })
              }
            }
          }
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'UPDATE' });

      const ctx = actor.getSnapshot().context;
      expect(ctx.str).toBe('new');
      expect(ctx.num).toBe(42);
      expect(ctx.bool).toBe(true);
      expect(ctx.obj).toEqual({ key: 'val2', extra: 100 });
    });
  });
});
