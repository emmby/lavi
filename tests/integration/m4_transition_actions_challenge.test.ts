import { describe, it, expect, vi } from 'vitest';
import {
  createMachine,
  createActor,
  setup,
  assign,
  raise,
  cancel,
  emit,
  log,
  enqueueActions,
  initialTransition,
  transition
} from '../../lavi-wasm/pkg-npm/lavi';

describe('Milestone 4 Pure Transitions & Action Capture Challenge', () => {
  describe('Dimension 1: Purity Guarantee & Assign Behavior', () => {
    it('1.1 initialTransition never invokes user action closures', () => {
      const rootEntryFn = vi.fn();
      const stateEntryFn = vi.fn();
      const namedSetupAction = vi.fn();
      const inlineObjAction = vi.fn();

      const machine = setup({
        actions: {
          namedSetupAction
        }
      }).createMachine({
        id: 'purity_initial',
        initial: 'first',
        context: { count: 1 },
        entry: [
          rootEntryFn,
          'namedSetupAction',
          {
            type: 'inlineObjAction',
            params: { test: true }
          }
        ],
        states: {
          first: {
            entry: [
              stateEntryFn,
              assign({ count: ({ context }) => context.count + 10 })
            ]
          }
        }
      });

      const [snapshot, actions] = initialTransition(machine);
      console.log('Test 1.1 captured actions:', JSON.stringify(actions));

      // Verify purity: ZERO action closures should have executed
      expect(rootEntryFn).not.toHaveBeenCalled();
      expect(stateEntryFn).not.toHaveBeenCalled();
      expect(namedSetupAction).not.toHaveBeenCalled();
      expect(inlineObjAction).not.toHaveBeenCalled();

      // Verify assign was evaluated purely
      expect(snapshot.context.count).toBe(11);
      expect(snapshot.value).toBe('first');

      // Verify actions were captured
      expect(actions.length).toBeGreaterThanOrEqual(2);
      expect(actions).toEqual(
        expect.arrayContaining([
          expect.objectContaining({ type: 'namedSetupAction' }),
          expect.objectContaining({
            type: 'inlineObjAction',
            params: { test: true }
          })
        ])
      );
    });

    it('2.1b enqueueActions receives context and event during pure transitions', () => {
      const machine = setup({}).createMachine({
        id: 'enqueue_ctx_evt',
        initial: 'idle',
        context: { count: 42 },
        states: {
          idle: {
            on: {
              TEST: {
                actions: enqueueActions(({ context, event, enqueue }) => {
                  enqueue({
                    type: 'captured',
                    params: {
                      c: context?.count,
                      e: event?.type
                    }
                  });
                })
              }
            }
          }
        }
      });

      const [s0] = initialTransition(machine);
      const [s1, actions] = transition(machine, s0, { type: 'TEST' });
      console.log('Test 2.1b captured actions:', JSON.stringify(actions));
      expect(actions).toContainEqual(
        expect.objectContaining({
          type: 'captured',
          params: { c: 42, e: 'TEST' }
        })
      );
    });

    it('1.2 transition never invokes user action closures across exit, transition, and entry', () => {
      const exitFn = vi.fn();
      const transitionFn = vi.fn();
      const targetEntryFn = vi.fn();
      const rootOnFn = vi.fn();
      const targetlessFn = vi.fn();

      const machine = createMachine({
        id: 'purity_transition',
        initial: 'source',
        context: { n: 0 },
        on: {
          ROOT_PING: {
            actions: rootOnFn
          }
        },
        states: {
          source: {
            exit: exitFn,
            on: {
              GO: {
                target: 'target',
                actions: [
                  transitionFn,
                  assign({ n: ({ context }) => context.n + 1 })
                ]
              },
              INTERNAL: {
                actions: [
                  targetlessFn,
                  assign({ n: ({ context }) => context.n + 5 })
                ]
              }
            }
          },
          target: {
            entry: targetEntryFn
          }
        }
      });

      const [s0] = initialTransition(machine);
      expect(s0.value).toBe('source');

      // Step 1: Internal targetless transition
      const [s1, a1] = transition(machine, s0, { type: 'INTERNAL' });
      expect(s1.value).toBe('source');
      expect(s1.context.n).toBe(5);
      expect(targetlessFn).not.toHaveBeenCalled();
      expect(exitFn).not.toHaveBeenCalled();
      expect(targetEntryFn).not.toHaveBeenCalled();

      // Step 2: Target transition (source -> target)
      const [s2, a2] = transition(machine, s1, { type: 'GO' });
      expect(s2.value).toBe('target');
      expect(s2.context.n).toBe(6);
      expect(exitFn).not.toHaveBeenCalled();
      expect(transitionFn).not.toHaveBeenCalled();
      expect(targetEntryFn).not.toHaveBeenCalled();

      // Step 3: Root event
      const [s3, a3] = transition(machine, s2, { type: 'ROOT_PING' });
      expect(rootOnFn).not.toHaveBeenCalled();
    });

    it('1.3 Sequential chained assigns evaluate updated context in sequence without side effects', () => {
      const machine = createMachine({
        id: 'chained_assigns',
        initial: 'idle',
        context: { a: 1, b: 0, c: 0 },
        states: {
          idle: {
            on: {
              CALC: {
                target: 'done',
                actions: [
                  assign({ a: ({ context }) => context.a + 9 }), // 1 + 9 = 10
                  assign({ b: ({ context }) => context.a * 2 }),  // 10 * 2 = 20
                  assign({ c: ({ context }) => context.a + context.b }) // 10 + 20 = 30
                ]
              }
            }
          },
          done: {}
        }
      });

      const [s0] = initialTransition(machine);
      const [s1, actions] = transition(machine, s0, { type: 'CALC' });
      console.log('Test 1.3 captured actions:', JSON.stringify(actions));

      expect(s1.context).toEqual({ a: 10, b: 20, c: 30 });
      expect(s1.value).toBe('done');
    });

    it('1.4 Sequential chained assigns via setup().createMachine', () => {
      const machine = setup({}).createMachine({
        id: 'chained_assigns_setup',
        initial: 'idle',
        context: { a: 1, b: 0, c: 0 },
        states: {
          idle: {
            on: {
              CALC: {
                target: 'done',
                actions: [
                  assign({ a: ({ context }) => context.a + 9 }),
                  assign({ b: ({ context }) => context.a * 2 }),
                  assign({ c: ({ context }) => context.a + context.b })
                ]
              }
            }
          },
          done: {}
        }
      });

      const [s0] = initialTransition(machine);
      const [s1] = transition(machine, s0, { type: 'CALC' });

      console.log('Test 1.4 s1.context:', JSON.stringify(s1.context));
      expect(s1.context).toEqual({ a: 10, b: 20, c: 30 });
      expect(s1.value).toBe('done');
    });
  });

  describe('Dimension 2: Action Capture Exhaustiveness', () => {
    it('2.1 Nested enqueueActions captures internal actions and applies assigns', () => {
      const outerFn = vi.fn();
      const innerFn = vi.fn();

      const machine = createMachine({
        id: 'nested_enqueue',
        initial: 'idle',
        context: { count: 0, logMessages: [] as string[] },
        states: {
          idle: {
            entry: enqueueActions((x) => {
              x.enqueue(assign({ count: 10 }));
              x.enqueue('outerAction');
              x.enqueue(
                enqueueActions((innerX) => {
                  innerX.enqueue(assign({ count: 20 }));
                  innerX.enqueue({ type: 'innerAction', params: { flag: true } });
                })
              );
              x.enqueue('finalAction');
            })
          }
        }
      });

      const [s0, actions] = initialTransition(machine);

      expect(s0.context.count).toBe(20);
      expect(actions).toEqual([
        expect.objectContaining({ type: 'outerAction' }),
        expect.objectContaining({ type: 'innerAction', params: { flag: true } }),
        expect.objectContaining({ type: 'finalAction' })
      ]);
    });

    it('2.2 Dynamic parameter actions evaluate against updated context and event payload', () => {
      const machine = createMachine({
        id: 'dynamic_params',
        initial: 'active',
        context: { factor: 5, total: 0 },
        states: {
          active: {
            on: {
              MULTIPLY: {
                target: 'active',
                actions: [
                  assign({ total: ({ context, event }) => event.value * context.factor }),
                  {
                    type: 'logResult',
                    params: ({ context, event }) => ({
                      result: context.total,
                      multiplier: event.value,
                      originalFactor: context.factor
                    })
                  }
                ]
              }
            }
          }
        }
      });

      const [s0] = initialTransition(machine);
      const [s1, actions] = transition(machine, s0, { type: 'MULTIPLY', value: 7 });

      expect(s1.context.total).toBe(35);
      expect(actions).toContainEqual(
        expect.objectContaining({
          type: 'logResult',
          params: {
            result: 35,
            multiplier: 7,
            originalFactor: 5
          }
        })
      );
    });

    it('2.3 Synthetic actions: delayed raise and delayed after', () => {
      const machine = createMachine({
        id: 'delays_machine',
        initial: 'step1',
        states: {
          step1: {
            entry: [
              raise({ type: 'IMMEDIATE' }),
              raise({ type: 'LATER' }, { delay: 150, id: 'laterId' })
            ],
            after: {
              300: 'step2'
            }
          },
          step2: {}
        }
      });

      const [s0, actions] = initialTransition(machine);

      // Verify immediate raise
      expect(actions).toContainEqual(
        expect.objectContaining({
          type: 'xstate.raise',
          params: expect.objectContaining({
            event: { type: 'IMMEDIATE' }
          })
        })
      );

      // Verify delayed raise
      expect(actions).toContainEqual(
        expect.objectContaining({
          type: 'xstate.raise',
          params: expect.objectContaining({
            delay: 150,
            id: 'laterId',
            event: { type: 'LATER' }
          })
        })
      );

      // Verify after synthetic raise
      expect(actions).toContainEqual(
        expect.objectContaining({
          type: 'xstate.raise',
          params: expect.objectContaining({
            delay: 300,
            event: { type: 'xstate.after.300.delays_machine.step1' }
          })
        })
      );
    });

    it('2.4 Cancel action capture', () => {
      const machine = createMachine({
        id: 'cancel_machine',
        initial: 'active',
        states: {
          active: {
            on: {
              ABORT: {
                target: 'cancelled',
                actions: cancel('activeTimer')
              }
            }
          },
          cancelled: {}
        }
      });

      const [s0] = initialTransition(machine);
      const [s1, actions] = transition(machine, s0, { type: 'ABORT' });

      expect(s1.value).toBe('cancelled');
      expect(actions).toContainEqual(
        expect.objectContaining({
          type: 'xstate.cancel',
          params: expect.objectContaining({
            sendId: 'activeTimer'
          })
        })
      );
    });

    it('2.5 Emit and Log actions capture with dynamic expressions', () => {
      const machine = createMachine({
        id: 'emit_log_machine',
        initial: 'running',
        context: { user: 'Alice', score: 99 },
        states: {
          running: {
            on: {
              FINISH: {
                target: 'done',
                actions: [
                  emit(({ context }) => ({
                    type: 'USER_COMPLETED',
                    user: context.user,
                    finalScore: context.score
                  })),
                  log(({ context }) => `User ${context.user} scored ${context.score}`)
                ]
              }
            }
          },
          done: {}
        }
      });

      const [s0] = initialTransition(machine);
      const [s1, actions] = transition(machine, s0, { type: 'FINISH' });

      expect(s1.value).toBe('done');
      expect(actions).toContainEqual(
        expect.objectContaining({
          type: 'xstate.emit',
          params: expect.objectContaining({
            event: {
              type: 'USER_COMPLETED',
              user: 'Alice',
              finalScore: 99
            }
          })
        })
      );

      expect(actions).toContainEqual(
        expect.objectContaining({
          type: 'xstate.log',
          params: expect.objectContaining({
            value: 'User Alice scored 99'
          })
        })
      );
    });
  });

  describe('Dimension 3: Chained Serverless State Simulation', () => {
    it('3.1 10 sequential microsteps with JSON serialization/resolveState match actor execution', () => {
      // Build a 10-step sequential pipeline machine: step0 -> step1 -> ... -> step9 -> finished
      const statesConfig: Record<string, any> = {};
      for (let i = 0; i < 10; i++) {
        const next = i === 9 ? 'finished' : `step${i + 1}`;
        statesConfig[`step${i}`] = {
          on: {
            NEXT: {
              target: next,
              actions: [
                assign({
                  stepsTaken: ({ context }) => [...context.stepsTaken, i],
                  totalValue: ({ context, event }) => context.totalValue + event.amount
                }),
                {
                  type: 'milestoneNotification',
                  params: ({ context }) => ({ step: i, currentTotal: context.totalValue })
                }
              ]
            }
          }
        };
      }
      statesConfig['finished'] = {
        type: 'final'
      };

      const machine = setup({}).createMachine({
        id: 'ten_step_pipeline',
        initial: 'step0',
        context: {
          stepsTaken: [] as number[],
          totalValue: 0
        },
        states: statesConfig
      });

      // Simulation A: Pure serverless transitions with full JSON roundtrip at each step
      let dbState: string;
      const recordedActions: any[][] = [];

      // Initial step
      const [initSnapshot, initActions] = initialTransition(machine);
      dbState = JSON.stringify(initSnapshot);
      recordedActions.push(initActions);

      // Execute 10 sequential events via resolveState(JSON.parse(dbState))
      for (let i = 0; i < 10; i++) {
        const reloadedSnapshot = machine.resolveState(JSON.parse(dbState));
        const [nextSnapshot, stepActions] = transition(machine, reloadedSnapshot, {
          type: 'NEXT',
          amount: (i + 1) * 10
        });

        // Persist to simulated database
        dbState = JSON.stringify(nextSnapshot);
        recordedActions.push(stepActions);
      }

      const finalServerlessSnapshot = JSON.parse(dbState);
      expect(finalServerlessSnapshot.value).toBe('finished');
      expect(finalServerlessSnapshot.status).toBe('done');
      expect(finalServerlessSnapshot.context.stepsTaken).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
      // sum( (i+1)*10 for i=0..9 ) = 10*(1+2+...+10) = 550
      expect(finalServerlessSnapshot.context.totalValue).toBe(550);

      // Verify that milestone actions were captured for each step
      for (let i = 0; i < 10; i++) {
        const stepActions = recordedActions[i + 1];
        expect(stepActions).toContainEqual(
          expect.objectContaining({
            type: 'milestoneNotification',
            params: expect.objectContaining({ step: i })
          })
        );
      }

      // Simulation B: Standard in-memory createActor run as oracle baseline
      const actor = createActor(machine).start();
      for (let i = 0; i < 10; i++) {
        actor.send({ type: 'NEXT', amount: (i + 1) * 10 });
      }
      const finalActorSnapshot = actor.getSnapshot();

      // Parity check: Serverless state resolution must match in-memory Actor snapshot
      expect(finalServerlessSnapshot.value).toEqual(finalActorSnapshot.value);
      expect(finalServerlessSnapshot.status).toEqual(finalActorSnapshot.status);
      expect(finalServerlessSnapshot.context).toEqual(finalActorSnapshot.context);
    });

    it('3.2 Chained serverless transitions across multiple branches with guards and assigns', () => {
      const machine = createMachine({
        id: 'branching_workflow',
        initial: 'triage',
        context: {
          category: '',
          approvals: 0
        },
        states: {
          triage: {
            on: {
              ROUTE_HIGH: {
                target: 'high_priority',
                actions: assign({ category: 'HIGH' })
              },
              ROUTE_LOW: {
                target: 'low_priority',
                actions: assign({ category: 'LOW' })
              }
            }
          },
          high_priority: {
            on: {
              APPROVE: {
                target: 'completed',
                actions: assign({ approvals: ({ context }) => context.approvals + 2 })
              }
            }
          },
          low_priority: {
            on: {
              APPROVE: {
                target: 'completed',
                actions: assign({ approvals: ({ context }) => context.approvals + 1 })
              }
            }
          },
          completed: {
            type: 'final'
          }
        }
      });

      // Branch 1: High priority
      let [snap1] = initialTransition(machine);
      let serialized = JSON.stringify(snap1);

      let [snap2] = transition(machine, machine.resolveState(JSON.parse(serialized)), { type: 'ROUTE_HIGH' });
      serialized = JSON.stringify(snap2);

      let [snap3] = transition(machine, machine.resolveState(JSON.parse(serialized)), { type: 'APPROVE' });
      serialized = JSON.stringify(snap3);

      const finalHigh = JSON.parse(serialized);
      expect(finalHigh.value).toBe('completed');
      expect(finalHigh.status).toBe('done');
      expect(finalHigh.context).toEqual({ category: 'HIGH', approvals: 2 });

      // Branch 2: Low priority
      [snap1] = initialTransition(machine);
      serialized = JSON.stringify(snap1);

      [snap2] = transition(machine, machine.resolveState(JSON.parse(serialized)), { type: 'ROUTE_LOW' });
      serialized = JSON.stringify(snap2);

      [snap3] = transition(machine, machine.resolveState(JSON.parse(serialized)), { type: 'APPROVE' });
      serialized = JSON.stringify(snap3);

      const finalLow = JSON.parse(serialized);
      expect(finalLow.value).toBe('completed');
      expect(finalLow.status).toBe('done');
      expect(finalLow.context).toEqual({ category: 'LOW', approvals: 1 });
    });

    it('3.3 Transition handles compound / hierarchical state values', () => {
      const machine = createMachine({
        id: 'hierarchical',
        initial: 'parent',
        states: {
          parent: {
            initial: 'child1',
            states: {
              child1: {
                on: {
                  NEXT: 'child2'
                }
              },
              child2: {}
            }
          }
        }
      });

      const [s0] = initialTransition(machine);
      console.log('Test 3.3 s0.value:', JSON.stringify(s0.value));
      expect(s0.value).toEqual({ parent: 'child1' });

      const [s1] = transition(machine, s0, { type: 'NEXT' });
      console.log('Test 3.3 s1.value:', JSON.stringify(s1.value));
      expect(s1.value).toEqual({ parent: 'child2' });
    });
  });

  describe('Dimension 4: Multi-Level Compound State Transitions', () => {
    it('4.1 Multi-level hierarchy initial resolution and deep transitions', () => {
      const exitL2 = vi.fn();
      const exitL1 = vi.fn();
      const entrySibling = vi.fn();

      const machine = createMachine({
        id: 'deep_compound',
        initial: 'l0',
        context: { depth: 0 },
        states: {
          l0: {
            initial: 'l1',
            states: {
              l1: {
                initial: 'l2',
                exit: exitL1,
                states: {
                  l2: {
                    initial: 'leaf_a',
                    exit: exitL2,
                    states: {
                      leaf_a: {
                        on: {
                          NEXT_LEAF: {
                            target: 'leaf_b',
                            actions: assign({ depth: 3 })
                          },
                          JUMP_OUT: {
                            target: 'sibling',
                            actions: assign({ depth: 1 })
                          }
                        }
                      },
                      leaf_b: {}
                    }
                  }
                }
              },
              sibling: {
                entry: entrySibling,
                on: {
                  RESET: {
                    target: 'l1',
                    actions: assign({ depth: 0 })
                  }
                }
              }
            }
          }
        }
      });

      // 1. Initial transition resolves all the way to leaf_a
      const [s0, a0] = initialTransition(machine);
      expect(s0.value).toEqual({ l0: { l1: { l2: 'leaf_a' } } });
      expect(s0.context.depth).toBe(0);

      // 2. Transition at deepest leaf level: leaf_a -> leaf_b
      const [s1, a1] = transition(machine, s0, { type: 'NEXT_LEAF' });
      expect(s1.value).toEqual({ l0: { l1: { l2: 'leaf_b' } } });
      expect(s1.context.depth).toBe(3);

      // 3. Jump from leaf_a up to sibling
      const [s2, a2] = transition(machine, s0, { type: 'JUMP_OUT' });
      expect(s2.value).toEqual({ l0: 'sibling' });
      expect(s2.context.depth).toBe(1);

      // Purity check: action closures for exit/entry should NOT be called
      expect(exitL2).not.toHaveBeenCalled();
      expect(exitL1).not.toHaveBeenCalled();
      expect(entrySibling).not.toHaveBeenCalled();

      // 4. Return from sibling back to l1, re-resolving initial states down to leaf_a
      const [s3, a3] = transition(machine, s2, { type: 'RESET' });
      expect(s3.value).toEqual({ l0: { l1: { l2: 'leaf_a' } } });
      expect(s3.context.depth).toBe(0);
    });
  });

  describe('Dimension 5: Parallel State Pure Stepping', () => {
    it('5.1 Parallel initial state resolution and orthogonal event dispatch', () => {
      const machine = createMachine({
        id: 'parallel_stepper',
        type: 'parallel',
        context: { audioVolume: 50, videoPlaying: false },
        states: {
          audio: {
            initial: 'muted',
            states: {
              muted: {
                on: {
                  UNMUTE: {
                    target: 'audible',
                    actions: [
                      assign({ audioVolume: 80 }),
                      { type: 'onUnmute', params: ({ context }) => ({ volume: context.audioVolume }) }
                    ]
                  }
                }
              },
              audible: {
                on: {
                  MUTE: {
                    target: 'muted',
                    actions: assign({ audioVolume: 0 })
                  }
                }
              }
            }
          },
          video: {
            initial: 'stopped',
            states: {
              stopped: {
                on: {
                  PLAY: {
                    target: 'playing',
                    actions: [
                      assign({ videoPlaying: true }),
                      { type: 'onPlayStarted' }
                    ]
                  }
                }
              },
              playing: {
                on: {
                  STOP: {
                    target: 'stopped',
                    actions: assign({ videoPlaying: false })
                  }
                }
              }
            }
          }
        }
      });

      // 1. Initial transition
      const [s0, a0] = initialTransition(machine);
      expect(s0.value).toEqual({ audio: 'muted', video: 'stopped' });
      expect(s0.context).toEqual({ audioVolume: 50, videoPlaying: false });

      // 2. Transition audio region only
      const [s1, a1] = transition(machine, s0, { type: 'UNMUTE' });
      expect(s1.value).toEqual({ audio: 'audible', video: 'stopped' });
      expect(s1.context.audioVolume).toBe(80);
      expect(s1.context.videoPlaying).toBe(false);
      expect(a1).toContainEqual(
        expect.objectContaining({
          type: 'onUnmute',
          params: { volume: 80 }
        })
      );

      // 3. Transition video region only from s1
      const [s2, a2] = transition(machine, s1, { type: 'PLAY' });
      expect(s2.value).toEqual({ audio: 'audible', video: 'playing' });
      expect(s2.context.videoPlaying).toBe(true);
      expect(a2).toContainEqual(
        expect.objectContaining({ type: 'onPlayStarted' })
      );

      // 4. Parity check with createActor
      const actor = createActor(machine).start();
      actor.send({ type: 'UNMUTE' });
      actor.send({ type: 'PLAY' });
      const actorSnap = actor.getSnapshot();
      expect(s2.value).toEqual(actorSnap.value);
      expect(s2.context).toEqual(actorSnap.context);
    });

    it('5.2 Parallel state transition on simultaneous event across regions', () => {
      const machine = createMachine({
        id: 'parallel_sync',
        type: 'parallel',
        context: { aHits: 0, bHits: 0 },
        states: {
          regionA: {
            initial: 'off',
            states: {
              off: {
                on: {
                  SYNC_PULSE: {
                    target: 'on',
                    actions: assign({ aHits: ({ context }) => context.aHits + 1 })
                  }
                }
              },
              on: {}
            }
          },
          regionB: {
            initial: 'idle',
            states: {
              idle: {
                on: {
                  SYNC_PULSE: {
                    target: 'active',
                    actions: assign({ bHits: ({ context }) => context.bHits + 2 })
                  }
                }
              },
              active: {}
            }
          }
        }
      });

      const [s0] = initialTransition(machine);
      expect(s0.value).toEqual({ regionA: 'off', regionB: 'idle' });

      // Send event that both regions listen to
      const [s1] = transition(machine, s0, { type: 'SYNC_PULSE' });
      expect(s1.value).toEqual({ regionA: 'on', regionB: 'active' });
      expect(s1.context).toEqual({ aHits: 1, bHits: 2 });
    });

    it('5.3 Nested parallel state initial transition and event dispatch', () => {
      const machine = createMachine({
        id: 'nested_parallel_stepper',
        initial: 'active',
        states: {
          active: {
            type: 'parallel',
            states: {
              reg1: {
                initial: 'ready',
                states: {
                  ready: {
                    on: {
                      GO_1: 'running'
                    }
                  },
                  running: {}
                }
              },
              reg2: {
                initial: 'idle',
                states: {
                  idle: {
                    on: {
                      GO_2: 'working'
                    }
                  },
                  working: {}
                }
              }
            }
          }
        }
      });

      const [s0] = initialTransition(machine);
      expect(s0.value).toEqual({
        active: {
          reg1: 'ready',
          reg2: 'idle'
        }
      });

      const [s1] = transition(machine, s0, { type: 'GO_1' });
      expect(s1.value).toEqual({
        active: {
          reg1: 'running',
          reg2: 'idle'
        }
      });

      const [s2] = transition(machine, s1, { type: 'GO_2' });
      expect(s2.value).toEqual({
        active: {
          reg1: 'running',
          reg2: 'working'
        }
      });
    });
  });

  describe('Dimension 6: Chained Assigns with Interleaved Dynamic Parameter Actions', () => {
    it('6.1 Interleaved dynamic parameter actions observe partially updated context', () => {
      const machine = createMachine({
        id: 'interleaved_chain',
        initial: 'idle',
        context: { x: 2, y: 3 },
        states: {
          idle: {
            on: {
              RUN: {
                target: 'done',
                actions: [
                  assign({ x: ({ context }) => context.x * 10 }), // x becomes 20
                  assign({ y: ({ context, event }) => context.x + event.add }), // y becomes 20 + 5 = 25
                  {
                    type: 'checkpoint1',
                    params: ({ context }) => ({ sum: context.x + context.y })
                  },
                  assign({ x: ({ context }) => context.y * 2 }), // x becomes 25 * 2 = 50
                  {
                    type: 'checkpoint2',
                    params: ({ context }) => ({ finalX: context.x, finalY: context.y })
                  }
                ]
              }
            }
          },
          done: {}
        }
      });

      const [s0] = initialTransition(machine);
      const [s1, actions] = transition(machine, s0, { type: 'RUN', add: 5 });

      expect(s1.value).toBe('done');
      expect(s1.context).toEqual({ x: 50, y: 25 });
      expect(actions).toEqual([
        expect.objectContaining({
          type: 'checkpoint1',
          params: { sum: 45 }
        }),
        expect.objectContaining({
          type: 'checkpoint2',
          params: { finalX: 50, finalY: 25 }
        })
      ]);
    });
  });

  describe('Dimension 7: Active After Delays & Synthetic DelayedRaise Feed-Forward', () => {
    it('7.1 Synthesized timer event drives state transition and schedules next timers', () => {
      const machine = createMachine({
        id: 'timer_pipeline',
        initial: 'phase1',
        states: {
          phase1: {
            after: {
              100: 'phase2'
            }
          },
          phase2: {
            after: {
              250: 'phase3'
            }
          },
          phase3: {
            type: 'final'
          }
        }
      });

      // 1. Initial transition produces phase1 with synthetic after event for 100ms
      const [s0, a0] = initialTransition(machine);
      expect(s0.value).toBe('phase1');
      expect(a0).toContainEqual(
        expect.objectContaining({
          type: 'xstate.raise',
          params: expect.objectContaining({
            delay: 100,
            event: { type: 'xstate.after.100.timer_pipeline.phase1' }
          })
        })
      );

      // 2. Feed the synthetic timer event into transition
      const timerEvt = a0.find(
        (a: any) => a.params?.event?.type === 'xstate.after.100.timer_pipeline.phase1'
      )!.params.event;

      const [s1, a1] = transition(machine, s0, timerEvt);
      expect(s1.value).toBe('phase2');
      expect(a1).toContainEqual(
        expect.objectContaining({
          type: 'xstate.raise',
          params: expect.objectContaining({
            delay: 250,
            event: { type: 'xstate.after.250.timer_pipeline.phase2' }
          })
        })
      );

      // 3. Feed the next timer event into transition
      const timerEvt2 = a1.find(
        (a: any) => a.params?.event?.type === 'xstate.after.250.timer_pipeline.phase2'
      )!.params.event;

      const [s2, a2] = transition(machine, s1, timerEvt2);
      expect(s2.value).toBe('phase3');
      expect(s2.status).toBe('done');
      // No more after delays scheduled
      expect(a2.filter((a: any) => a.type === 'xstate.raise' && a.params?.delay)).toHaveLength(0);
    });
  });

  describe('Dimension 8: enqueueActions Helper Rigorous Inspection', () => {
    it('8.1 enqueueActions helper receives self and enables conditional enqueuing', () => {
      let inspectedSelf: any = null;

      const machine = createMachine({
        id: 'enqueue_rich_helper',
        initial: 'ready',
        context: { threshold: 10, items: [] as string[] },
        states: {
          ready: {
            on: {
              CHECK: {
                actions: enqueueActions(({ context, event, enqueue, self }) => {
                  inspectedSelf = self;
                  if (event.val > context.threshold) {
                    enqueue(assign({ items: ({ context }) => [...context.items, `high:${event.val}`] }));
                    enqueue({
                      type: 'notifyThreshold',
                      params: ({ context }) => ({ count: context.items.length, status: 'HIGH' })
                    });
                  } else {
                    enqueue(assign({ items: ({ context }) => [...context.items, `low:${event.val}`] }));
                    enqueue({
                      type: 'notifyThreshold',
                      params: ({ context }) => ({ count: context.items.length, status: 'LOW' })
                    });
                  }
                })
              }
            }
          }
        }
      });

      const [s0] = initialTransition(machine);

      // Low branch
      const [s1, a1] = transition(machine, s0, { type: 'CHECK', val: 5 });
      expect(s1.context.items).toEqual(['low:5']);
      expect(a1).toContainEqual(
        expect.objectContaining({
          type: 'notifyThreshold',
          params: { count: 1, status: 'LOW' }
        })
      );
      expect(inspectedSelf).toBeDefined();

      // High branch
      const [s2, a2] = transition(machine, s1, { type: 'CHECK', val: 15 });
      expect(s2.context.items).toEqual(['low:5', 'high:15']);
      expect(a2).toContainEqual(
        expect.objectContaining({
          type: 'notifyThreshold',
          params: { count: 2, status: 'HIGH' }
        })
      );
    });
  });
});
