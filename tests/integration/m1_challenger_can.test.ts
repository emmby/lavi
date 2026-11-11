import { describe, it, expect, vi } from 'vitest';
import {
  createMachine,
  createActor,
  assign,
  and,
  or,
  not,
  fromCallback,
  fromPromise
} from '../../lavi-wasm/pkg-npm/lavi';

describe('Milestone 1 Challenger: can() Action Suppression & State Machine Invariants', () => {
  // =========================================================================
  // Dimension 1: Strict Zero Side Effects & Complete Action Suppression
  // =========================================================================
  describe('Dimension 1: Action Suppression & Zero Side Effects', () => {
    it('1.1 suppresses root, compound, and leaf entry/exit/transition actions during can()', () => {
      const rootEntry = vi.fn();
      const rootExit = vi.fn();
      const parentEntry = vi.fn();
      const parentExit = vi.fn();
      const child1Entry = vi.fn();
      const child1Exit = vi.fn();
      const child2Entry = vi.fn();
      const child2Exit = vi.fn();
      const transitionAction = vi.fn();
      const assignAction = vi.fn();

      const machine = createMachine({
        id: 'nested_actions',
        context: { count: 0, flags: [] },
        entry: [rootEntry],
        exit: [rootExit],
        initial: 'parent1',
        states: {
          parent1: {
            entry: [parentEntry],
            exit: [parentExit],
            initial: 'child1',
            states: {
              child1: {
                entry: [child1Entry],
                exit: [child1Exit],
                on: {
                  NEXT: {
                    target: '#nested_actions.parent2.child2',
                    actions: [
                      transitionAction,
                      assign(({ context }: any) => {
                        assignAction();
                        return { count: context.count + 1 };
                      })
                    ]
                  }
                }
              }
            }
          },
          parent2: {
            entry: [vi.fn()],
            exit: [vi.fn()],
            initial: 'child2',
            states: {
              child2: {
                entry: [child2Entry],
                exit: [child2Exit]
              }
            }
          }
        }
      });

      const actor = createActor(machine).start();

      // Clear initial entry mocks
      rootEntry.mockClear();
      parentEntry.mockClear();
      child1Entry.mockClear();

      const snapshotBefore = actor.getSnapshot();
      expect(snapshotBefore.context).toEqual({ count: 0, flags: [] });

      // Call can() multiple times
      expect(snapshotBefore.can({ type: 'NEXT' })).toBe(true);
      expect(actor.getSnapshot().can({ type: 'NEXT' })).toBe(true);
      expect(actor.getSnapshot().can({ type: 'NON_EXISTENT' })).toBe(false);

      // Verify ZERO actions were executed
      expect(rootEntry).not.toHaveBeenCalled();
      expect(rootExit).not.toHaveBeenCalled();
      expect(parentEntry).not.toHaveBeenCalled();
      expect(parentExit).not.toHaveBeenCalled();
      expect(child1Exit).not.toHaveBeenCalled();
      expect(child2Entry).not.toHaveBeenCalled();
      expect(transitionAction).not.toHaveBeenCalled();
      expect(assignAction).not.toHaveBeenCalled();

      // Verify context and state are completely unchanged
      const snapshotAfter = actor.getSnapshot();
      expect(snapshotAfter.value).toEqual({ parent1: 'child1' });
      expect(snapshotAfter.context).toEqual({ count: 0, flags: [] });
    });

    it('1.2 suppresses actor spawning (fromCallback and fromPromise) inside assign during can()', () => {
      let callbackSpawned = false;
      let promiseSpawned = false;

      const machine = createMachine({
        id: 'spawn_suppression',
        initial: 'idle',
        context: { childRef: null },
        states: {
          idle: {
            on: {
              SPAWN_CALLBACK: {
                target: 'spawned',
                actions: assign(({ spawn }: any) => ({
                  childRef: spawn(
                    fromCallback(() => {
                      callbackSpawned = true;
                    })
                  )
                }))
              },
              SPAWN_PROMISE: {
                target: 'spawned',
                actions: assign(({ spawn }: any) => ({
                  childRef: spawn(
                    fromPromise(async () => {
                      promiseSpawned = true;
                      return 'done';
                    })
                  )
                }))
              }
            }
          },
          spawned: {}
        }
      });

      const actor = createActor(machine).start();

      expect(actor.getSnapshot().can({ type: 'SPAWN_CALLBACK' })).toBe(true);
      expect(actor.getSnapshot().can({ type: 'SPAWN_PROMISE' })).toBe(true);

      expect(callbackSpawned).toBe(false);
      expect(promiseSpawned).toBe(false);
      expect(actor.getSnapshot().context.childRef).toBeNull();
    });

    it('1.3 suppresses raise actions during can() without contaminating internal event queues', () => {
      const raiseAction = vi.fn();

      const machine = createMachine({
        id: 'raise_suppression',
        initial: 'step1',
        states: {
          step1: {
            on: {
              TRIGGER: {
                target: 'step2',
                actions: [
                  raiseAction,
                  { type: 'xstate.raise', params: { event: { type: 'AUTO_TRIGGER' } } }
                ]
              }
            }
          },
          step2: {
            on: {
              AUTO_TRIGGER: 'step3'
            }
          },
          step3: {}
        }
      });

      const actor = createActor(machine).start();

      expect(actor.getSnapshot().can({ type: 'TRIGGER' })).toBe(true);
      expect(raiseAction).not.toHaveBeenCalled();

      // State must still be step1, not step2 or step3
      expect(actor.getSnapshot().value).toBe('step1');

      // Now send an unrelated event to verify queue wasn't contaminated
      actor.send({ type: 'UNKNOWN_EVENT' });
      expect(actor.getSnapshot().value).toBe('step1');
    });
  });

  // =========================================================================
  // Dimension 2: Hierarchies, Compound States, and Transition Precedence
  // =========================================================================
  describe('Dimension 2: Deep Hierarchies and Transition Precedence', () => {
    it('2.1 accurately resolves candidate transitions from innermost leaf to root', () => {
      const machine = createMachine({
        id: 'hierarchy',
        initial: 'level1',
        on: {
          GLOBAL_EVT: '.level1.level2.level3.b'
        },
        states: {
          level1: {
            initial: 'level2',
            on: {
              LEVEL1_EVT: '.level2.level3.b'
            },
            states: {
              level2: {
                initial: 'level3',
                on: {
                  LEVEL2_EVT: '.level3.b'
                },
                states: {
                  level3: {
                    initial: 'a',
                    states: {
                      a: {
                        on: {
                          LEAF_EVT: 'b'
                        }
                      },
                      b: {}
                    }
                  }
                }
              }
            }
          }
        }
      });

      const actor = createActor(machine).start();
      const snapshot = actor.getSnapshot();

      // All ancestor and leaf events should be valid
      expect(snapshot.can({ type: 'LEAF_EVT' })).toBe(true);
      expect(snapshot.can({ type: 'LEVEL2_EVT' })).toBe(true);
      expect(snapshot.can({ type: 'LEVEL1_EVT' })).toBe(true);
      expect(snapshot.can({ type: 'GLOBAL_EVT' })).toBe(true);

      // Unrelated events
      expect(snapshot.can({ type: 'UNMATCHED' })).toBe(false);
    });

    it('2.2 respects child-over-parent precedence when child guard is true, and falls back to parent when child guard is false', () => {
      let childGuardCallCount = 0;
      let parentGuardCallCount = 0;

      const machine = createMachine({
        id: 'precedence',
        initial: 'parent',
        states: {
          parent: {
            initial: 'child',
            on: {
              CONFLICT: {
                target: 'target_parent',
                guard: ({ context }: any) => {
                  parentGuardCallCount++;
                  return context.allowParent;
                }
              }
            },
            states: {
              child: {
                on: {
                  CONFLICT: {
                    target: '#precedence.target_child',
                    guard: ({ context }: any) => {
                      childGuardCallCount++;
                      return context.allowChild;
                    }
                  }
                }
              }
            }
          },
          target_parent: {},
          target_child: {}
        }
      });

      // Case A: Child guard true, parent guard false -> true (child takes precedence)
      const actorA = createActor(machine, {
        context: { allowChild: true, allowParent: false }
      }).start();
      expect(actorA.getSnapshot().can({ type: 'CONFLICT' })).toBe(true);

      // Case B: Child guard false, parent guard true -> true (falls back to parent)
      const actorB = createActor(machine, {
        context: { allowChild: false, allowParent: true }
      }).start();
      expect(actorB.getSnapshot().can({ type: 'CONFLICT' })).toBe(true);

      // Case C: Both guards false -> false
      const actorC = createActor(machine, {
        context: { allowChild: false, allowParent: false }
      }).start();
      expect(actorC.getSnapshot().can({ type: 'CONFLICT' })).toBe(false);
    });
  });

  // =========================================================================
  // Dimension 3: Parallel States with Independent and Shared Events
  // =========================================================================
  describe('Dimension 3: Parallel States', () => {
    it('3.1 evaluates can() correctly across independent and joint parallel regions', () => {
      const machine = createMachine({
        id: 'parallel_can',
        type: 'parallel',
        states: {
          nav: {
            initial: 'home',
            states: {
              home: { on: { GO_SETTINGS: 'settings' } },
              settings: { on: { GO_HOME: 'home' } }
            }
          },
          audio: {
            initial: 'stopped',
            states: {
              stopped: { on: { PLAY: 'playing' } },
              playing: { on: { PAUSE: 'paused', STOP: 'stopped' } },
              paused: { on: { RESUME: 'playing', STOP: 'stopped' } }
            }
          },
          sync: {
            initial: 'idle',
            states: {
              idle: { on: { REFRESH: 'syncing' } },
              syncing: { on: { CANCEL: 'idle' } }
            }
          }
        }
      });

      const actor = createActor(machine).start();
      const snapshot = actor.getSnapshot();

      // Events targeting region 1
      expect(snapshot.can({ type: 'GO_SETTINGS' })).toBe(true);
      expect(snapshot.can({ type: 'GO_HOME' })).toBe(false); // already in home

      // Events targeting region 2
      expect(snapshot.can({ type: 'PLAY' })).toBe(true);
      expect(snapshot.can({ type: 'PAUSE' })).toBe(false); // stopped, cannot pause
      expect(snapshot.can({ type: 'STOP' })).toBe(false);

      // Events targeting region 3
      expect(snapshot.can({ type: 'REFRESH' })).toBe(true);
      expect(snapshot.can({ type: 'CANCEL' })).toBe(false);

      // Non-existent event
      expect(snapshot.can({ type: 'SHUTDOWN' })).toBe(false);
    });

    it('3.2 evaluates can() when an event is accepted in one region but guarded out in another', () => {
      const machine = createMachine({
        id: 'parallel_mixed_guards',
        type: 'parallel',
        states: {
          r1: {
            initial: 'active',
            states: {
              active: {
                on: {
                  MULTI: {
                    target: 'done',
                    guard: () => false // disallowed in r1
                  }
                }
              },
              done: {}
            }
          },
          r2: {
            initial: 'active',
            states: {
              active: {
                on: {
                  MULTI: {
                    target: 'done',
                    guard: () => true // allowed in r2
                  }
                }
              },
              done: {}
            }
          }
        }
      });

      const actor = createActor(machine).start();
      // Because r2 can transition on MULTI, the machine as a whole can accept MULTI
      expect(actor.getSnapshot().can({ type: 'MULTI' })).toBe(true);
    });
  });

  // =========================================================================
  // Dimension 4: Multiple Guards and Compound Boolean Operators
  // =========================================================================
  describe('Dimension 4: Multiple Guards and Guard Combinators', () => {
    it('4.1 evaluates transition candidates with first-match guard evaluation', () => {
      const machine = createMachine({
        id: 'guard_candidates',
        initial: 'select',
        states: {
          select: {
            on: {
              CHOOSE: [
                { target: 'optA', guard: ({ event }: any) => event.choice === 'A' },
                { target: 'optB', guard: ({ event }: any) => event.choice === 'B' },
                { target: 'optC', guard: ({ event }: any) => event.choice === 'C' }
              ]
            }
          },
          optA: {},
          optB: {},
          optC: {}
        }
      });

      const actor = createActor(machine).start();
      const snap = actor.getSnapshot();

      expect(snap.can({ type: 'CHOOSE', choice: 'A' })).toBe(true);
      expect(snap.can({ type: 'CHOOSE', choice: 'B' })).toBe(true);
      expect(snap.can({ type: 'CHOOSE', choice: 'C' })).toBe(true);
      expect(snap.can({ type: 'CHOOSE', choice: 'D' })).toBe(false);
      expect(snap.can({ type: 'CHOOSE' })).toBe(false);
    });

    it('4.2 supports compound combinators: and, or, not during can()', () => {
      const machine = createMachine(
        {
          id: 'guard_combinators',
          initial: 'idle',
          states: {
            idle: {
              on: {
                TEST_AND: {
                  target: 'ok',
                  guard: and(['isOnline', 'isAdmin'])
                },
                TEST_OR: {
                  target: 'ok',
                  guard: or(['isOnline', 'isAdmin'])
                },
                TEST_NOT: {
                  target: 'ok',
                  guard: not('isBlocked')
                }
              }
            },
            ok: {}
          }
        },
        {
          guards: {
            isOnline: ({ context }: any) => Boolean(context.online),
            isAdmin: ({ context }: any) => Boolean(context.admin),
            isBlocked: ({ context }: any) => Boolean(context.blocked)
          }
        }
      );

      // Case 1: online=true, admin=false, blocked=false
      const a1 = createActor(machine, { context: { online: true, admin: false, blocked: false } }).start();
      expect(a1.getSnapshot().can({ type: 'TEST_AND' })).toBe(false);
      expect(a1.getSnapshot().can({ type: 'TEST_OR' })).toBe(true);
      expect(a1.getSnapshot().can({ type: 'TEST_NOT' })).toBe(true);

      // Case 2: online=true, admin=true, blocked=true
      const a2 = createActor(machine, { context: { online: true, admin: true, blocked: true } }).start();
      expect(a2.getSnapshot().can({ type: 'TEST_AND' })).toBe(true);
      expect(a2.getSnapshot().can({ type: 'TEST_OR' })).toBe(true);
      expect(a2.getSnapshot().can({ type: 'TEST_NOT' })).toBe(false);
    });
  });

  // =========================================================================
  // Dimension 5: Delayed Transitions (after) and Timer Immunity
  // =========================================================================
  describe('Dimension 5: Delayed Transitions & Timer Immunity', () => {
    it('5.1 calling can() does not reset, cancel, or trigger active after timers', () => {
      const afterEntry = vi.fn();
      const timeoutTarget = vi.fn();

      const machine = createMachine({
        id: 'timer_immunity',
        initial: 'waiting',
        states: {
          waiting: {
            entry: afterEntry,
            after: {
              10000: {
                target: 'timed_out',
                actions: [timeoutTarget]
              }
            },
            on: {
              MANUAL_ESCAPE: 'escaped'
            }
          },
          timed_out: {},
          escaped: {}
        }
      });

      const actor = createActor(machine).start();

      // Check can() on manual escape
      expect(actor.getSnapshot().can({ type: 'MANUAL_ESCAPE' })).toBe(true);
      expect(actor.getSnapshot().can({ type: 'RANDOM' })).toBe(false);

      // Verify actions untouched
      expect(timeoutTarget).not.toHaveBeenCalled();
      expect(actor.getSnapshot().value).toBe('waiting');
    });
  });

  // =========================================================================
  // Dimension 6: Stopped and Done Machine Lifecycle Invariants
  // =========================================================================
  describe('Dimension 6: Stopped and Done Lifecycle Invariants', () => {
    it('6.1 stopped machines strictly return false for can() on all events', () => {
      const machine = createMachine({
        id: 'stopped_check',
        initial: 'ready',
        on: {
          GLOBAL: 'ready'
        },
        states: {
          ready: {
            on: {
              CLICK: 'ready',
              ANY: 'ready'
            }
          }
        }
      });

      const actor = createActor(machine).start();
      expect(actor.getSnapshot().can({ type: 'CLICK' })).toBe(true);
      expect(actor.getSnapshot().can({ type: 'GLOBAL' })).toBe(true);

      // Explicitly stop the actor
      actor.stop();

      const stoppedSnapshot = actor.getSnapshot();
      expect(stoppedSnapshot.status).toBe('stopped');

      // Stopped actor snapshot MUST return false for all events
      expect(stoppedSnapshot.can({ type: 'CLICK' })).toBe(false);
      expect(stoppedSnapshot.can({ type: 'GLOBAL' })).toBe(false);
      expect(stoppedSnapshot.can({ type: 'ANY' })).toBe(false);
      expect(stoppedSnapshot.can({ type: 'RANDOM' })).toBe(false);
    });

    it('6.2 unstarted actors return true for valid transitions without executing actions', () => {
      const entryAction = vi.fn();
      const transitionAction = vi.fn();

      const machine = createMachine({
        id: 'unstarted_check',
        initial: 'start',
        states: {
          start: {
            entry: entryAction,
            on: {
              PROCEED: {
                target: 'next',
                actions: transitionAction
              }
            }
          },
          next: {}
        }
      });

      const unstartedActor = createActor(machine);
      const snapshot = unstartedActor.getSnapshot();

      expect(snapshot.can({ type: 'PROCEED' })).toBe(true);
      expect(snapshot.can({ type: 'INVALID' })).toBe(false);

      expect(entryAction).not.toHaveBeenCalled();
      expect(transitionAction).not.toHaveBeenCalled();
    });

    it('6.3 machine reaching final state returns false for can() when no transitions exist', () => {
      const machine = createMachine({
        id: 'final_check',
        initial: 'running',
        states: {
          running: {
            on: { FINISH: 'completed' }
          },
          completed: {
            type: 'final'
          }
        }
      });

      const actor = createActor(machine).start();
      expect(actor.getSnapshot().can({ type: 'FINISH' })).toBe(true);

      actor.send({ type: 'FINISH' });
      const doneSnapshot = actor.getSnapshot();
      expect(doneSnapshot.status).toBe('done');
      expect(doneSnapshot.value).toBe('completed');

      // Final state has no outgoing transitions
      expect(doneSnapshot.can({ type: 'FINISH' })).toBe(false);
      expect(doneSnapshot.can({ type: 'ANYTHING' })).toBe(false);
    });
  });

  // =========================================================================
  // Dimension 7: In-Memory Isolation under High-Frequency Hammering
  // =========================================================================
  describe('Dimension 7: High-Frequency In-Memory Hammering Stress Test', () => {
    it('7.1 maintains strict idempotency and zero mutations over 1,000 rapid can() checks', () => {
      const counter = { reads: 0 };
      const machine = createMachine({
        id: 'hammer',
        context: { items: [1, 2, 3], nested: { value: 42 } },
        initial: 'active',
        states: {
          active: {
            on: {
              QUERY: {
                target: 'active',
                guard: ({ context }: any) => {
                  counter.reads++;
                  return context.nested.value === 42;
                },
                actions: assign({
                  items: [999] // Should NEVER be applied
                })
              }
            }
          }
        }
      });

      const actor = createActor(machine).start();
      const initialSnapshot = actor.getSnapshot();
      const initialContextJson = JSON.stringify(initialSnapshot.context);

      for (let i = 0; i < 1000; i++) {
        const canResult = initialSnapshot.can({ type: 'QUERY' });
        expect(canResult).toBe(true);
      }

      // 1000 guard evaluations
      expect(counter.reads).toBe(1000);

      // Verify zero mutation in snapshot context
      expect(JSON.stringify(actor.getSnapshot().context)).toBe(initialContextJson);
      expect(actor.getSnapshot().context.items).toEqual([1, 2, 3]);
    });
  });

  // =========================================================================
  // Dimension 8: StateNode & Synthetic Invoke Transition Invariants
  // =========================================================================
  describe('Dimension 8: StateNode & Invoke Deduplication Invariants', () => {
    it('8.1 getStateNodeById resolves correctly and transitions map contains synthetic events', () => {
      const machine = createMachine({
        id: 'invoke_check',
        initial: 'fetching',
        states: {
          fetching: {
            id: 'fetch_state',
            invoke: {
              src: 'fetchData',
              onDone: 'success',
              onError: 'failure'
            },
            on: {
              CANCEL: 'idle'
            }
          },
          idle: {},
          success: {},
          failure: {}
        }
      });

      // getStateNodeById by id and by name
      const byId = machine.getStateNodeById('fetch_state');
      expect(byId).toBeDefined();

      const transitionsMap = byId.transitions;
      expect(transitionsMap).toBeDefined();
      expect(transitionsMap.get('CANCEL')).toBeDefined();
      expect(transitionsMap.get('xstate.done.actor.0.fetch_state')).toBeDefined();
      expect(transitionsMap.get('xstate.error.actor.0.fetch_state')).toBeDefined();

      // Serialization check: onDone/onError must not duplicate in JSON
      const serialized = JSON.parse(JSON.stringify(machine));
      expect(serialized.states.fetching.invoke[0].onDone).toBeUndefined();
      expect(serialized.states.fetching.invoke[0].onError).toBeUndefined();
      expect(serialized.states.fetching.on['xstate.done.actor.0.fetch_state']).toBeDefined();
      expect(serialized.states.fetching.on['xstate.error.actor.0.fetch_state']).toBeDefined();
    });

    it('8.2 user-defined onDone / onError transition takes precedence over synthetic transition', () => {
      const machine = createMachine({
        id: 'dedup_check',
        initial: 'load',
        states: {
          load: {
            id: 'load_state',
            invoke: {
              src: 'loader',
              onDone: 'fallback_target'
            },
            on: {
              // Explicit user transition with custom target
              'xstate.done.actor.0.load_state': {
                target: 'user_target'
              }
            }
          },
          fallback_target: {},
          user_target: {}
        }
      });

      const node = machine.getStateNodeById('load_state');
      const doneTrans = node.transitions.get('xstate.done.actor.0.load_state');
      expect(doneTrans).toBeDefined();
      expect(doneTrans.length).toBe(1);
      // Target must be user_target, not duplicated or overwritten with fallback_target
      expect(doneTrans[0].target).toEqual(['#dedup_check.user_target']);
    });
  });

  // =========================================================================
  // Dimension 9: Targetless & Internal Transitions
  // =========================================================================
  describe('Dimension 9: Targetless & Internal Transitions', () => {
    it('9.1 reports true for targetless transitions without executing internal actions', () => {
      const actionSpy = vi.fn();
      const machine = createMachine({
        id: 'targetless',
        initial: 'active',
        states: {
          active: {
            on: {
              PING: {
                actions: actionSpy
              }
            }
          }
        }
      });

      const actor = createActor(machine).start();
      expect(actor.getSnapshot().can({ type: 'PING' })).toBe(true);
      expect(actionSpy).not.toHaveBeenCalled();
      expect(actor.getSnapshot().value).toBe('active');
    });
  });

  // =========================================================================
  // Dimension 10: Reentering Transitions
  // =========================================================================
  describe('Dimension 10: Reentering Transitions', () => {
    it('10.1 reports true for reentering transitions without executing entry, exit, or transition actions', () => {
      const entrySpy = vi.fn();
      const exitSpy = vi.fn();
      const transSpy = vi.fn();

      const machine = createMachine({
        id: 'reenter',
        initial: 'active',
        states: {
          active: {
            entry: entrySpy,
            exit: exitSpy,
            on: {
              RELOAD: {
                target: 'active',
                reenter: true,
                actions: transSpy
              }
            }
          }
        }
      });

      const actor = createActor(machine).start();
      entrySpy.mockClear();

      expect(actor.getSnapshot().can({ type: 'RELOAD' })).toBe(true);
      expect(entrySpy).not.toHaveBeenCalled();
      expect(exitSpy).not.toHaveBeenCalled();
      expect(transSpy).not.toHaveBeenCalled();
    });
  });

  // =========================================================================
  // Dimension 11: Wildcard & Prefix Event Descriptors
  // =========================================================================
  describe('Dimension 11: Wildcard and Prefix Event Descriptors', () => {
    it('11.1 matches wildcard and prefix event descriptors properly during can()', () => {
      const machine = createMachine({
        id: 'wildcards',
        initial: 'listening',
        states: {
          listening: {
            on: {
              'mouse.*': 'mouse_handled',
              '*': 'catch_all'
            }
          },
          mouse_handled: {},
          catch_all: {}
        }
      });

      const actor = createActor(machine).start();
      const snap = actor.getSnapshot();

      expect(snap.can({ type: 'mouse.click' })).toBe(true);
      expect(snap.can({ type: 'mouse.move' })).toBe(true);
      expect(snap.can({ type: 'keyboard.keypress' })).toBe(true);
      expect(snap.can({ type: 'custom.event' })).toBe(true);
      expect(snap.can({ type: '' })).toBe(false);
    });
  });

  // =========================================================================
  // Dimension 12: Guard Context & Event Payload Precision
  // =========================================================================
  describe('Dimension 12: Guard Context and Event Payload Inspection', () => {
    it('12.1 correctly provides full event payload and live context to guards during can()', () => {
      let receivedCtx: any = null;
      let receivedEvt: any = null;

      const machine = createMachine({
        id: 'payload_check',
        context: { role: 'admin', authVersion: 3 },
        initial: 'check',
        states: {
          check: {
            on: {
              VERIFY: {
                target: 'verified',
                guard: ({ context, event }: any) => {
                  receivedCtx = context;
                  receivedEvt = event;
                  return context.role === 'admin' && event.token === 'SECRET_123';
                }
              }
            }
          },
          verified: {}
        }
      });

      const actor = createActor(machine).start();

      const canInvalid = actor.getSnapshot().can({ type: 'VERIFY', token: 'WRONG' });
      expect(canInvalid).toBe(false);
      expect(receivedCtx).toEqual({ role: 'admin', authVersion: 3 });
      expect(receivedEvt.token).toBe('WRONG');

      const canValid = actor.getSnapshot().can({ type: 'VERIFY', token: 'SECRET_123' });
      expect(canValid).toBe(true);
      expect(receivedEvt.token).toBe('SECRET_123');
    });
  });

  // =========================================================================
  // Dimension 13: Malformed & Boundary Event Inputs
  // =========================================================================
  describe('Dimension 13: Malformed and Boundary Event Inputs', () => {
    it('13.1 handles malformed and edge-case event arguments gracefully without throwing', () => {
      const machine = createMachine({
        id: 'boundary_inputs',
        initial: 'idle',
        states: {
          idle: {
            on: {
              VALID: 'idle'
            }
          }
        }
      });

      const actor = createActor(machine).start();
      const snap = actor.getSnapshot();

      // Empty string
      expect(snap.can('')).toBe(false);
      expect(snap.can({ type: '' })).toBe(false);

      // Non-existent
      expect(snap.can('INVALID')).toBe(false);
      expect(snap.can({ type: 'INVALID' })).toBe(false);

      // String event format (XState allows string or object)
      expect(snap.can('VALID')).toBe(true);
      expect(snap.can({ type: 'VALID' })).toBe(true);
    });
  });
});
