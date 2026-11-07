import { describe, it, expect, vi } from 'vitest';
import {
  createMachine,
  createActor,
  setup,
  assign,
  enqueueActions,
  emit,
  log,
  transition,
  initialTransition
} from '../../lavi-wasm/pkg-npm/lavi';

describe('Milestone 4 Empirical Challenger Suite (Tags, Meta, Input & Pure Transitions)', () => {
  // =========================================================================
  // Area 1: Adversarial Tag Stress Testing
  // =========================================================================
  describe('Area 1: Adversarial Tag Stress Testing', () => {
    it('1.1 aggregates tags across 12 levels of deep hierarchy and verifies leaf-to-root collection', () => {
      // Build a 12-level nested machine
      const machine = createMachine({
        id: 'deepMachine',
        tags: ['rootTag'],
        initial: 'l1',
        states: {
          l1: {
            tags: ['tag1'],
            initial: 'l2',
            states: {
              l2: {
                initial: 'l3',
                states: {
                  l3: {
                    tags: ['tag3'],
                    initial: 'l4',
                    states: {
                      l4: {
                        initial: 'l5',
                        states: {
                          l5: {
                            tags: ['tag5'],
                            initial: 'l6',
                            states: {
                              l6: {
                                initial: 'l7',
                                states: {
                                  l7: {
                                    tags: ['tag7'],
                                    initial: 'l8',
                                    states: {
                                      l8: {
                                        initial: 'l9',
                                        states: {
                                          l9: {
                                            tags: ['tag9'],
                                            initial: 'l10',
                                            states: {
                                              l10: {
                                                initial: 'l11',
                                                states: {
                                                  l11: {
                                                    tags: ['tag11'],
                                                    initial: 'l12',
                                                    states: {
                                                      l12: {
                                                        tags: ['leafTag'],
                                                        on: {
                                                          JUMP_OUT: '#deepMachine.l1_alt'
                                                        }
                                                      }
                                                    }
                                                  }
                                                }
                                              }
                                            }
                                          }
                                        }
                                      }
                                    }
                                  }
                                }
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
          },
          l1_alt: {
            tags: ['altTag']
          }
        }
      });

      const actor = createActor(machine).start();
      const snapshot = actor.getSnapshot();

      // Verify all active tags are present
      expect(snapshot.hasTag('rootTag')).toBe(true);
      expect(snapshot.hasTag('tag1')).toBe(true);
      expect(snapshot.hasTag('tag3')).toBe(true);
      expect(snapshot.hasTag('tag5')).toBe(true);
      expect(snapshot.hasTag('tag7')).toBe(true);
      expect(snapshot.hasTag('tag9')).toBe(true);
      expect(snapshot.hasTag('tag11')).toBe(true);
      expect(snapshot.hasTag('leafTag')).toBe(true);

      // Verify inactive tags are absent
      expect(snapshot.hasTag('altTag')).toBe(false);
      expect(snapshot.hasTag('nonexistent')).toBe(false);

      // Verify Set equality
      expect(snapshot.tags).toEqual(
        new Set(['rootTag', 'tag1', 'tag3', 'tag5', 'tag7', 'tag9', 'tag11', 'leafTag'])
      );

      // Transition out of the deep hierarchy
      actor.send({ type: 'JUMP_OUT' });
      const altSnapshot = actor.getSnapshot();

      // Deep tags must be removed; rootTag and altTag must be present
      expect(altSnapshot.hasTag('rootTag')).toBe(true);
      expect(altSnapshot.hasTag('altTag')).toBe(true);
      expect(altSnapshot.hasTag('tag1')).toBe(false);
      expect(altSnapshot.hasTag('leafTag')).toBe(false);
      expect(altSnapshot.tags).toEqual(new Set(['rootTag', 'altTag']));
    });

    it('1.2 manages parallel states with duplicate, disjoint, and transition-varying tags', () => {
      const machine = createMachine({
        type: 'parallel',
        tags: 'globalParallelTag', // string tag on root
        states: {
          regionA: {
            tags: ['sharedTag', 'tagA'],
            initial: 'a1',
            states: {
              a1: {
                tags: 'leafA1',
                on: { NEXT_A: 'a2' }
              },
              a2: {
                tags: ['leafA2', 'sharedTag'] // duplicate of parent tag
              }
            }
          },
          regionB: {
            tags: ['sharedTag', 'tagB'],
            initial: 'b1',
            states: {
              b1: {
                tags: ['leafB1'],
                on: { NEXT_B: 'b2' }
              },
              b2: {
                tags: ['leafB2', 'exclusiveB2']
              }
            }
          },
          regionC: {
            initial: 'c1',
            states: {
              c1: {
                on: { NEXT_C: 'c2' }
              },
              c2: {
                tags: ['tagC2']
              }
            }
          }
        }
      });

      const actor = createActor(machine).start();
      let snap = actor.getSnapshot();

      // Initial state: regionA=a1, regionB=b1, regionC=c1
      expect(snap.hasTag('globalParallelTag')).toBe(true);
      expect(snap.hasTag('sharedTag')).toBe(true);
      expect(snap.hasTag('tagA')).toBe(true);
      expect(snap.hasTag('tagB')).toBe(true);
      expect(snap.hasTag('leafA1')).toBe(true);
      expect(snap.hasTag('leafB1')).toBe(true);
      expect(snap.hasTag('exclusiveB2')).toBe(false);
      expect(snap.hasTag('tagC2')).toBe(false);

      expect(snap.tags).toEqual(
        new Set(['globalParallelTag', 'sharedTag', 'tagA', 'leafA1', 'tagB', 'leafB1'])
      );

      // Transition only region A to a2
      actor.send({ type: 'NEXT_A' });
      snap = actor.getSnapshot();
      expect(snap.hasTag('leafA1')).toBe(false);
      expect(snap.hasTag('leafA2')).toBe(true);
      expect(snap.hasTag('sharedTag')).toBe(true); // still active in both regions
      expect(snap.tags).toEqual(
        new Set(['globalParallelTag', 'sharedTag', 'tagA', 'leafA2', 'tagB', 'leafB1'])
      );

      // Transition region B to b2
      actor.send({ type: 'NEXT_B' });
      snap = actor.getSnapshot();
      expect(snap.hasTag('leafB1')).toBe(false);
      expect(snap.hasTag('leafB2')).toBe(true);
      expect(snap.hasTag('exclusiveB2')).toBe(true);
      expect(snap.tags).toEqual(
        new Set(['globalParallelTag', 'sharedTag', 'tagA', 'leafA2', 'tagB', 'leafB2', 'exclusiveB2'])
      );

      // Transition region C to c2
      actor.send({ type: 'NEXT_C' });
      snap = actor.getSnapshot();
      expect(snap.hasTag('tagC2')).toBe(true);
      expect(snap.tags.has('tagC2')).toBe(true);
    });

    it('1.3 strictly maintains snapshot.hasTag(t) === snapshot.tags.has(t) across edge queries', () => {
      const machine = createMachine({
        initial: 'active',
        tags: ['system_ready', 'payment_pending'],
        states: {
          active: {
            tags: ['user_authenticated', 'tier_1']
          }
        }
      });

      const actor = createActor(machine).start();
      const snap = actor.getSnapshot();

      // Test matching tags
      for (const t of ['system_ready', 'payment_pending', 'user_authenticated', 'tier_1']) {
        expect(snap.hasTag(t)).toBe(true);
        expect(snap.tags.has(t)).toBe(true);
      }

      // Test non-matching substring tags (ensure no prefix matching false positives)
      for (const t of ['system', 'ready', 'payment', 'pending', 'user', 'tier', '1', '']) {
        expect(snap.hasTag(t)).toBe(false);
        expect(snap.tags.has(t)).toBe(false);
      }
    });
  });

  // =========================================================================
  // Area 2: Metadata Aggregation Stress Testing
  // =========================================================================
  describe('Area 2: Metadata Aggregation Stress Testing', () => {
    it('2.1 aggregates deep metadata with root ID, custom state IDs, and default (machine) ID', () => {
      const machineWithRootId = createMachine({
        id: 'checkout',
        meta: { flow: 'ecommerce', version: 2 },
        initial: 'cart',
        states: {
          cart: {
            meta: { step: 1, editable: true },
            initial: 'review',
            states: {
              review: {
                id: 'customReviewId',
                meta: { subview: 'cart_review' },
                initial: 'items',
                states: {
                  items: {
                    meta: { list: 'items_list' }
                  }
                }
              }
            }
          }
        }
      });

      const actor = createActor(machineWithRootId).start();
      const meta = actor.getSnapshot().getMeta();

      expect(meta).toEqual({
        checkout: { flow: 'ecommerce', version: 2 },
        'checkout.cart': { step: 1, editable: true },
        customReviewId: { subview: 'cart_review' },
        'checkout.cart.review.items': { list: 'items_list' }
      });

      // Now test with default (machine) ID (no id specified at root)
      const machineWithoutRootId = createMachine({
        meta: { defaultRoot: true },
        initial: 'stepA',
        states: {
          stepA: {
            meta: { step: 'A' },
            initial: 'inner',
            states: {
              inner: {
                id: 'myExplicitInnerId',
                meta: { innerData: 42 }
              }
            }
          }
        }
      });

      const actor2 = createActor(machineWithoutRootId).start();
      const meta2 = actor2.getSnapshot().getMeta();

      expect(meta2).toEqual({
        '(machine)': { defaultRoot: true },
        '(machine).stepA': { step: 'A' },
        myExplicitInnerId: { innerData: 42 }
      });
    });

    it('2.2 strictly isolates inactive sibling metadata across complex branch transitions', () => {
      const machine = createMachine({
        id: 'wizard',
        meta: { title: 'Setup Wizard' },
        initial: 'step1',
        states: {
          step1: {
            meta: { stepIndex: 1, guide: 'Welcome to step 1' },
            on: { GOTO_2: 'step2' }
          },
          step2: {
            meta: { stepIndex: 2, guide: 'Configure preferences' },
            initial: 'basic',
            states: {
              basic: {
                meta: { submode: 'basic_pref' },
                on: { ADVANCED: 'advanced' }
              },
              advanced: {
                meta: { submode: 'advanced_pref' },
                on: { GOTO_3: '#wizard.step3' }
              }
            }
          },
          step3: {
            meta: { stepIndex: 3, guide: 'Finish and review' }
          }
        }
      });

      const actor = createActor(machine).start();

      // Step 1: only wizard and wizard.step1
      let meta = actor.getSnapshot().getMeta();
      expect(meta).toEqual({
        wizard: { title: 'Setup Wizard' },
        'wizard.step1': { stepIndex: 1, guide: 'Welcome to step 1' }
      });
      expect(meta['wizard.step2']).toBeUndefined();
      expect(meta['wizard.step3']).toBeUndefined();

      // Transition to Step 2 basic
      actor.send({ type: 'GOTO_2' });
      meta = actor.getSnapshot().getMeta();
      expect(meta).toEqual({
        wizard: { title: 'Setup Wizard' },
        'wizard.step2': { stepIndex: 2, guide: 'Configure preferences' },
        'wizard.step2.basic': { submode: 'basic_pref' }
      });
      expect(meta['wizard.step1']).toBeUndefined();
      expect(meta['wizard.step2.advanced']).toBeUndefined();
      expect(meta['wizard.step3']).toBeUndefined();

      // Transition to Step 2 advanced
      actor.send({ type: 'ADVANCED' });
      meta = actor.getSnapshot().getMeta();
      expect(meta).toEqual({
        wizard: { title: 'Setup Wizard' },
        'wizard.step2': { stepIndex: 2, guide: 'Configure preferences' },
        'wizard.step2.advanced': { submode: 'advanced_pref' }
      });
      expect(meta['wizard.step2.basic']).toBeUndefined();

      // Transition to Step 3
      actor.send({ type: 'GOTO_3' });
      meta = actor.getSnapshot().getMeta();
      expect(meta).toEqual({
        wizard: { title: 'Setup Wizard' },
        'wizard.step3': { stepIndex: 3, guide: 'Finish and review' }
      });
      expect(meta['wizard.step1']).toBeUndefined();
      expect(meta['wizard.step2']).toBeUndefined();
      expect(meta['wizard.step2.advanced']).toBeUndefined();
    });

    it('2.3 dynamically calculates metadata on machine.resolveState() before actor creation and persists through transitions', () => {
      const machine = createMachine({
        id: 'order',
        meta: { domain: 'sales' },
        initial: 'pending',
        states: {
          pending: {
            meta: { statusText: 'Order Pending' },
            on: { PAY: 'paid' }
          },
          paid: {
            meta: { statusText: 'Order Paid in Full' },
            initial: 'packing',
            states: {
              packing: {
                meta: { warehouse: 'Building B' },
                on: { SHIP: 'shipped' }
              },
              shipped: {
                meta: { trackingAvailable: true }
              }
            }
          }
        }
      });

      // Test resolveState directly
      const resolvedPacking = machine.resolveState({ value: { paid: 'packing' } });
      expect(resolvedPacking.getMeta()).toEqual({
        order: { domain: 'sales' },
        'order.paid': { statusText: 'Order Paid in Full' },
        'order.paid.packing': { warehouse: 'Building B' }
      });

      // Start actor from resolved snapshot
      const actor = createActor(machine, { snapshot: resolvedPacking }).start();
      expect(actor.getSnapshot().getMeta()).toEqual({
        order: { domain: 'sales' },
        'order.paid': { statusText: 'Order Paid in Full' },
        'order.paid.packing': { warehouse: 'Building B' }
      });

      // Transition to shipped
      actor.send({ type: 'SHIP' });
      expect(actor.getSnapshot().getMeta()).toEqual({
        order: { domain: 'sales' },
        'order.paid': { statusText: 'Order Paid in Full' },
        'order.paid.shipped': { trackingAvailable: true }
      });
    });

    it('2.4 aggregates metadata across multiple concurrent active regions in a parallel state', () => {
      const machine = createMachine({
        id: 'dashboard',
        type: 'parallel',
        meta: { app: 'analytics' },
        states: {
          userPanel: {
            meta: { panel: 'users' },
            initial: 'online',
            states: {
              online: {
                meta: { count: 120 }
              },
              offline: {
                meta: { count: 0 }
              }
            }
          },
          metricsPanel: {
            id: 'metricsId',
            meta: { panel: 'metrics' },
            initial: 'cpu',
            states: {
              cpu: {
                meta: { load: 'high' }
              },
              memory: {
                meta: { used: '64%' }
              }
            }
          }
        }
      });

      const actor = createActor(machine).start();
      const meta = actor.getSnapshot().getMeta();

      expect(meta).toEqual({
        dashboard: { app: 'analytics' },
        'dashboard.userPanel': { panel: 'users' },
        'dashboard.userPanel.online': { count: 120 },
        metricsId: { panel: 'metrics' },
        'dashboard.metricsPanel.cpu': { load: 'high' }
      });
      expect(meta['dashboard.userPanel.offline']).toBeUndefined();
      expect(meta['dashboard.metricsPanel.memory']).toBeUndefined();
    });

    it('2.5 supports non-object metadata types (arrays, strings, numbers, booleans)', () => {
      const machine = createMachine({
        id: 'multiMeta',
        meta: 'rootStringMeta',
        initial: 's1',
        states: {
          s1: {
            meta: ['array', 123, true],
            initial: 's2',
            states: {
              s2: {
                meta: 42.5
              }
            }
          }
        }
      });

      const actor = createActor(machine).start();
      const meta = actor.getSnapshot().getMeta();

      expect(meta).toEqual({
        multiMeta: 'rootStringMeta',
        'multiMeta.s1': ['array', 123, true],
        'multiMeta.s1.s2': 42.5
      });
    });
  });

  // =========================================================================
  // Area 3: Input & Context Lifecycle Stress Testing
  // =========================================================================
  describe('Area 3: Input & Context Lifecycle Stress Testing', () => {
    it('3.1 handles deeply nested input objects with primitives, arrays, nulls, and floats without corruption', () => {
      const complexInput = {
        name: 'test_run',
        count: 99,
        ratio: 3.14159,
        flag: true,
        nothing: null,
        emptyArr: [],
        mixedArr: [1, 'two', { inner: 'three' }, [4, 5]],
        nested: {
          a: {
            b: {
              c: {
                val: 'deep_val',
                arr: [{ id: 101 }, { id: 102 }]
              }
            }
          }
        }
      };

      const machine = createMachine({
        context: ({ input }: any) => ({
          receivedInput: input,
          sum: input.count + 1,
          extracted: input.nested.a.b.c.val
        }),
        initial: 'ready',
        states: {
          ready: {}
        }
      });

      const actor = createActor(machine, { input: complexInput }).start();
      const snap = actor.getSnapshot();

      expect(snap.status).toBe('active');
      expect((snap.context as any).receivedInput).toEqual(complexInput);
      expect((snap.context as any).sum).toBe(100);
      expect((snap.context as any).extracted).toBe('deep_val');
    });

    it('3.2 captures throwing context factories (custom Error, string, object) into snapshot.error and status: error', () => {
      // 1. Throw custom Error instance
      const machineThrowingError = createMachine({
        context: ({ input }: any) => {
          if (!input || !input.requiredField) {
            throw new Error('MISSING_REQUIRED_FIELD');
          }
          return { data: input.requiredField };
        }
      });

      const actor1 = createActor(machineThrowingError);
      const snap1 = actor1.getSnapshot();
      expect(snap1.status).toBe('error');
      expect((snap1.error as Error).message).toBe('MISSING_REQUIRED_FIELD');

      // Calling start() on errored actor should be a safe no-op
      actor1.start();
      expect(actor1.getSnapshot().status).toBe('error');

      // 2. Throw primitive string
      const machineThrowingString = createMachine({
        context: () => {
          throw 'FATAL_STRING_ERROR';
        }
      });
      const actor2 = createActor(machineThrowingString);
      const snap2 = actor2.getSnapshot();
      expect(snap2.status).toBe('error');
      expect(snap2.error).toBe('FATAL_STRING_ERROR');

      // 3. Throw structured object
      const machineThrowingObject = createMachine({
        context: () => {
          throw { code: 500, detail: 'DATABASE_TIMEOUT' };
        }
      });
      const actor3 = createActor(machineThrowingObject);
      const snap3 = actor3.getSnapshot();
      expect(snap3.status).toBe('error');
      expect(snap3.error).toEqual({ code: 500, detail: 'DATABASE_TIMEOUT' });
    });

    it('3.3 passes event.input identity to entry actions and initial assignments', () => {
      let rootEntryReceivedInput: any;
      let stateEntryReceivedInput: any;
      let assignReceivedInput: any;

      const machine = createMachine({
        context: ({ input }: any) => ({
          base: input.startValue
        }),
        entry: ({ event }: any) => {
          rootEntryReceivedInput = event?.input;
        },
        initial: 'init',
        states: {
          init: {
            entry: [
              assign({
                assignedFromEventInput: ({ event }: any) => event?.input?.multiplier * 10
              }),
              ({ event }: any) => {
                stateEntryReceivedInput = event?.input;
              }
            ]
          }
        }
      });

      const testInput = { startValue: 5, multiplier: 3 };
      const actor = createActor(machine, { input: testInput }).start();

      expect(rootEntryReceivedInput).toEqual(testInput);
      expect(stateEntryReceivedInput).toEqual(testInput);
      expect((actor.getSnapshot().context as any).base).toBe(5);
      expect((actor.getSnapshot().context as any).assignedFromEventInput).toBe(30);
    });

    it('3.4 handles null input and undefined input gracefully', () => {
      const machine = createMachine({
        context: ({ input }: any) => ({
          hasInput: input !== undefined,
          isNull: input === null
        }),
        initial: 'check',
        states: { check: {} }
      });

      const actorNull = createActor(machine, { input: null }).start();
      expect(actorNull.getSnapshot().context).toEqual({
        hasInput: true,
        isNull: true
      });

      const actorUndefined = createActor(machine).start();
      expect(actorUndefined.getSnapshot().context).toEqual({
        hasInput: false,
        isNull: false
      });
    });
  });

  // =========================================================================
  // Area 4: Pure Transition & Action Resolution Stress Testing
  // =========================================================================
  describe('Area 4: Pure Transition & Action Resolution Stress Testing', () => {
    it('4.1a verifies enqueueActions receives context and event during actor execution', () => {
      let capturedContext: any;
      let capturedEvent: any;
      let enqueuedActionExecuted = false;

      const machine = createMachine({
        context: { count: 10, threshold: 5 },
        initial: 'idle',
        states: {
          idle: {
            on: {
              STEP: {
                target: 'active',
                actions: enqueueActions(({ enqueue, context, event }: any) => {
                  capturedContext = context;
                  capturedEvent = event;
                  if (event.payload > context.threshold) {
                    enqueue(() => {
                      enqueuedActionExecuted = true;
                    });
                  }
                })
              }
            }
          },
          active: {}
        }
      });

      createActor(machine).start().send({ type: 'STEP', payload: 12 });

      expect(capturedContext).toEqual({ count: 10, threshold: 5 });
      expect(capturedEvent).toEqual({ type: 'STEP', payload: 12 });
      expect(enqueuedActionExecuted).toBe(true);
    });

    it('4.1b resolves static enqueueActions in pure transition without reading context/event', () => {
      const machine = createMachine({
        initial: 'idle',
        states: {
          idle: {
            on: {
              STEP: {
                target: 'active',
                actions: [
                  enqueueActions(({ enqueue }: any) => {
                    enqueue({ type: 'customAction', params: { flag: true } });
                    enqueue('simpleAction');
                  })
                ]
              }
            }
          },
          active: {}
        }
      });

      const [initialState] = initialTransition(machine);
      const [nextState, actions] = transition(
        machine,
        initialState,
        { type: 'STEP' }
      );

      expect(nextState.value).toBe('active');
      expect(actions).toEqual([
        expect.objectContaining({
          type: 'customAction',
          params: { flag: true }
        }),
        expect.objectContaining({
          type: 'simpleAction'
        })
      ]);
    });

    it('4.1c [DEFECT FINDING] verifies enqueueActions in pure transition fails if accessing context or event', () => {
      let contextWasUndefined = false;
      const machine = createMachine({
        context: { count: 10 },
        initial: 'idle',
        states: {
          idle: {
            on: {
              STEP: {
                target: 'active',
                actions: [
                  enqueueActions(({ enqueue, context }: any) => {
                    if (context === undefined) {
                      contextWasUndefined = true;
                    }
                    enqueue({ type: 'actionWithContext', params: { count: context?.count } });
                  })
                ]
              }
            }
          },
          active: {}
        }
      });

      const [initialState] = initialTransition(machine);
      const [nextState, actions] = transition(machine, initialState, { type: 'STEP' });

      expect(nextState.value).toBe('active');
      expect(contextWasUndefined).toBe(false);
      expect(actions).toEqual([
        expect.objectContaining({
          type: 'actionWithContext',
          params: { count: 10 }
        })
      ]);
    });

    it('4.2 resolves emit and log actions in pure transition without executing side-effects', () => {
      const machine = createMachine({
        context: { count: 42 },
        initial: 'waiting',
        states: {
          waiting: {
            on: {
              NOTIFY: {
                target: 'notified',
                actions: [
                  emit({ type: 'STATUS_UPDATE', payload: 'all_ok' }),
                  log(({ context }: any) => `Current count: ${context.count}`)
                ]
              }
            }
          },
          notified: {}
        }
      });

      const [state0] = initialTransition(machine);
      const [state1, actions] = transition(machine, state0, { type: 'NOTIFY' });

      expect(state1.value).toBe('notified');
      expect(actions).toEqual([
        expect.objectContaining({
          type: 'xstate.emit',
          params: expect.objectContaining({
            event: { type: 'STATUS_UPDATE', payload: 'all_ok' }
          })
        }),
        expect.objectContaining({
          type: 'xstate.log',
          params: expect.objectContaining({
            value: 'Current count: 42'
          })
        })
      ]);
    });
  });
});
