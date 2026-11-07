import { describe, it, expect, vi } from 'vitest';
import { createMachine, createActor, assign } from '../../lavi-wasm/pkg-npm/lavi';

describe('Milestone 2 Iteration 2 Challenger Suite (WASM Lifecycle & Cascade Invariants)', () => {
  it('6-level deep onDone cascade preserves ancestor timer and isolates root exit', async () => {
    const rootExitSpy = vi.fn();
    const timeoutSpy = vi.fn();

    const machine = createMachine({
      id: 'root6_wasm',
      initial: 'l1',
      exit: rootExitSpy,
      states: {
        l1: {
          initial: 'l2',
          after: {
            60: { target: 'timed_out', actions: timeoutSpy }
          },
          states: {
            l2: {
              initial: 'l3',
              states: {
                l3: {
                  initial: 'l4',
                  states: {
                    l4: {
                      initial: 'l5',
                      states: {
                        l5: {
                          initial: 'l6',
                          states: {
                            l6: {
                              initial: 'step_a',
                              states: {
                                step_a: {
                                  on: { FINISH_L6: 'l6_final' }
                                },
                                l6_final: { type: 'final' }
                              },
                              onDone: 'l5_wait'
                            }
                          }
                        },
                        l5_wait: {
                          on: { FINISH_L5: 'l5_final' }
                        },
                        l5_final: { type: 'final' }
                      },
                      onDone: 'l4_final'
                    },
                    l4_final: { type: 'final' }
                  },
                  onDone: 'l3_final'
                },
                l3_final: { type: 'final' }
              },
              onDone: 'l2_wait'
            },
            l2_wait: {
              on: { FINISH_L2: 'l2_final' }
            },
            l2_final: { type: 'final' }
          },
          onDone: 'all_done'
        },
        timed_out: {
          type: 'final'
        },
        all_done: {
          type: 'final'
        }
      }
    });

    const actor = createActor(machine).start();
    expect(actor.getSnapshot().status).toBe('active');
    expect(actor.getSnapshot().matches({ l1: { l2: { l3: { l4: { l5: { l6: 'step_a' } } } } } })).toBe(true);

    // Step 1: Finish L6. Cascades to l5_wait
    actor.send({ type: 'FINISH_L6' });
    expect(actor.getSnapshot().status).toBe('active');
    expect(rootExitSpy).not.toHaveBeenCalled();

    // Step 2: Finish L5. Cascades to l2_wait
    actor.send({ type: 'FINISH_L5' });
    expect(actor.getSnapshot().status).toBe('active');
    expect(rootExitSpy).not.toHaveBeenCalled();

    // Step 3: Wait for ancestor 60ms timer to fire
    await new Promise<void>((resolve, reject) => {
      setTimeout(() => {
        try {
          expect(actor.getSnapshot().status).toBe('done');
          expect(actor.getSnapshot().value).toBe('timed_out');
          expect(timeoutSpy).toHaveBeenCalled();
          expect(rootExitSpy).toHaveBeenCalledTimes(1);
          resolve();
        } catch (err) {
          reject(err);
        }
      }, 120);
    });
  });

  it('Root parallel machine with 3 regions and nested parallel/compound substates', () => {
    const rootExitSpy = vi.fn();

    const machine = createMachine({
      id: 'root_par3_wasm',
      type: 'parallel',
      exit: rootExitSpy,
      states: {
        r1: {
          initial: 'comp',
          states: {
            comp: {
              initial: 'working',
              states: {
                working: { on: { R1_FINISH_INNER: 'inner_done' } },
                inner_done: { type: 'final' }
              },
              onDone: 'comp_done'
            },
            comp_done: {
              type: 'final'
            }
          }
        },
        r2: {
          initial: 'sub_par',
          states: {
            sub_par: {
              type: 'parallel',
              states: {
                pA: {
                  initial: 's',
                  states: {
                    s: { on: { R2_A_DONE: 'd' } },
                    d: { type: 'final' }
                  }
                },
                pB: {
                  initial: 's',
                  states: {
                    s: { on: { R2_B_DONE: 'd' } },
                    d: { type: 'final' }
                  }
                }
              },
              onDone: 'r2_done'
            },
            r2_done: {
              type: 'final'
            }
          }
        },
        r3: {
          initial: 'idle',
          states: {
            idle: {
              on: { R3_DONE: 'finished' }
            },
            finished: {
              type: 'final'
            }
          }
        }
      }
    });

    const actor = createActor(machine).start();
    expect(actor.getSnapshot().status).toBe('active');

    // 1. R1 completes inner substate -> comp_done (R1 final)
    actor.send({ type: 'R1_FINISH_INNER' });
    expect(actor.getSnapshot().status).toBe('active');
    expect(rootExitSpy).not.toHaveBeenCalled();

    // 2. R2 completes pA only
    actor.send({ type: 'R2_A_DONE' });
    expect(actor.getSnapshot().status).toBe('active');
    expect(rootExitSpy).not.toHaveBeenCalled();

    // 3. R2 completes pB -> sub_par completes to r2_done (R2 final)
    actor.send({ type: 'R2_B_DONE' });
    expect(actor.getSnapshot().status).toBe('active');
    expect(rootExitSpy).not.toHaveBeenCalled();

    // 4. Complete R3 -> finished (R3 final)
    actor.send({ type: 'R3_DONE' });

    // Now all 3 regions are final
    expect(actor.getSnapshot().status).toBe('done');
    expect(rootExitSpy).toHaveBeenCalledTimes(1);
  });

  it('Root re-entry targeting "#" without and with reenter flag', () => {
    const trace: string[] = [];

    const machine = createMachine({
      entry: () => trace.push('root_entry'),
      exit: () => trace.push('root_exit'),
      initial: 'step1',
      states: {
        step1: {
          entry: () => trace.push('step1_entry'),
          exit: () => trace.push('step1_exit'),
          on: {
            NEXT: 'step2'
          }
        },
        step2: {
          entry: () => trace.push('step2_entry'),
          exit: () => trace.push('step2_exit'),
          on: {
            RESET_NO_REENTER: '#',
            RESET_WITH_REENTER: {
              target: '#',
              reenter: true
            }
          }
        }
      }
    });

    const actor = createActor(machine).start();
    expect(trace).toEqual(['root_entry', 'step1_entry']);
    trace.length = 0;

    // Transition to step2
    actor.send({ type: 'NEXT' });
    expect(trace).toEqual(['step1_exit', 'step2_entry']);
    trace.length = 0;

    // 1. Reset without reenter: root is LCCA, so root does not exit/reenter
    actor.send({ type: 'RESET_NO_REENTER' });
    expect(actor.getSnapshot().value).toBe('step1');
    expect(trace).toEqual(['step2_exit', 'step1_entry']);
    trace.length = 0;

    // Transition to step2 again
    actor.send({ type: 'NEXT' });
    trace.length = 0;

    // 2. Reset with reenter: true -> root exits and reenters
    actor.send({ type: 'RESET_WITH_REENTER' });
    expect(actor.getSnapshot().value).toBe('step1');
    expect(trace).toEqual(['step2_exit', 'root_exit', 'root_entry', 'step1_entry']);
  });

  it('Cascading onDone with context guards selecting proper target branch', () => {
    const machine = createMachine({
      id: 'guarded_cascade_wasm',
      initial: 'processing',
      context: { score: 0 },
      states: {
        processing: {
          initial: 'sub',
          states: {
            sub: {
              initial: 'eval',
              states: {
                eval: {
                  on: {
                    SET_HIGH: {
                      target: 'eval_done',
                      actions: assign({ score: 95 })
                    },
                    SET_LOW: {
                      target: 'eval_done',
                      actions: assign({ score: 40 })
                    }
                  }
                },
                eval_done: {
                  type: 'final'
                }
              },
              onDone: [
                {
                  guard: ({ context }: any) => context.score >= 80,
                  target: '#guarded_cascade_wasm.passed'
                },
                {
                  target: '#guarded_cascade_wasm.failed'
                }
              ]
            }
          }
        },
        passed: { type: 'final' },
        failed: { type: 'final' }
      }
    });

    // Test Run 1: High score
    const actor1 = createActor(machine).start();
    actor1.send({ type: 'SET_HIGH' });
    expect(actor1.getSnapshot().value).toBe('passed');
    expect(actor1.getSnapshot().status).toBe('done');

    // Test Run 2: Low score
    const actor2 = createActor(machine).start();
    actor2.send({ type: 'SET_LOW' });
    expect(actor2.getSnapshot().value).toBe('failed');
    expect(actor2.getSnapshot().status).toBe('done');
  });
});
