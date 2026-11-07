import { describe, it, expect, vi } from 'vitest';
import { createMachine, createActor } from '../../lavi-wasm/pkg-npm/lavi';

describe('Milestone 2 Empirical Challenger Suite (WASM & JS)', () => {
  // =========================================================================
  // 1. Multi-Level Cascading onDone Events
  // =========================================================================
  describe('Multi-Level Cascading onDone', () => {
    it('cascades 3 levels of onDone cleanly in a single turn', () => {
      const trace: string[] = [];

      const machine = createMachine({
        id: 'cascade3',
        initial: 'parent',
        states: {
          parent: {
            initial: 'child',
            states: {
              child: {
                initial: 'grandchild',
                states: {
                  grandchild: {
                    on: { FINISH: 'grandchild_done' }
                  },
                  grandchild_done: {
                    type: 'final'
                  }
                },
                onDone: {
                  target: 'child_done',
                  actions: () => {
                    trace.push('child_onDone');
                  }
                }
              },
              child_done: {
                type: 'final'
              }
            },
            onDone: {
              target: 'parent_done',
              actions: () => {
                trace.push('parent_onDone');
              }
            }
          },
          parent_done: {
            type: 'final'
          }
        }
      });

      const actor = createActor(machine).start();
      expect(actor.getSnapshot().value).toEqual({
        parent: {
          child: 'grandchild'
        }
      });
      expect(actor.getSnapshot().status).toBe('active');

      actor.send({ type: 'FINISH' });

      expect(trace).toEqual(['child_onDone', 'parent_onDone']);
      expect(actor.getSnapshot().value).toBe('parent_done');
      expect(actor.getSnapshot().status).toBe('done');
    });

    it('cascades 4 levels of onDone with output payloads passing through', () => {
      const trace: any[] = [];

      const machine = createMachine({
        id: 'cascade4_out',
        initial: 'l1',
        states: {
          l1: {
            initial: 'l2',
            states: {
              l2: {
                initial: 'l3',
                states: {
                  l3: {
                    initial: 'l4',
                    states: {
                      l4: {
                        on: { GO: 'l4_done' }
                      },
                      l4_done: {
                        type: 'final',
                        output: { level: 4 }
                      }
                    },
                    onDone: {
                      target: 'l3_done',
                      actions: ({ event }) => {
                        trace.push({ step: 'l3', output: (event as any).output });
                      }
                    }
                  },
                  l3_done: {
                    type: 'final',
                    output: { level: 3 }
                  }
                },
                onDone: {
                  target: 'l2_done',
                  actions: ({ event }) => {
                    trace.push({ step: 'l2', output: (event as any).output });
                  }
                }
              },
              l2_done: {
                type: 'final',
                output: { level: 2 }
              }
            },
            onDone: {
              target: 'l1_done',
              actions: ({ event }) => {
                trace.push({ step: 'l1', output: (event as any).output });
              }
            }
          },
          l1_done: {
            type: 'final',
            output: { level: 1 }
          }
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'GO' });

      expect(actor.getSnapshot().value).toBe('l1_done');
      expect(actor.getSnapshot().status).toBe('done');
      expect(trace).toEqual([
        { step: 'l3', output: { level: 4 } },
        { step: 'l2', output: { level: 3 } },
        { step: 'l1', output: { level: 2 } }
      ]);
    });
  });

  // =========================================================================
  // 2. Parallel State Completion Synchronization
  // =========================================================================
  describe('Parallel State Completion Synchronization', () => {
    it('synchronizes 4 parallel regions completing sequentially', () => {
      let doneCount = 0;
      const machine = createMachine({
        id: 'par4_seq',
        initial: 'active',
        states: {
          active: {
            type: 'parallel',
            states: {
              r1: {
                initial: 'start',
                states: {
                  start: { on: { F1: 'done' } },
                  done: { type: 'final' }
                }
              },
              r2: {
                initial: 'start',
                states: {
                  start: { on: { F2: 'done' } },
                  done: { type: 'final' }
                }
              },
              r3: {
                initial: 'start',
                states: {
                  start: { on: { F3: 'done' } },
                  done: { type: 'final' }
                }
              },
              r4: {
                initial: 'start',
                states: {
                  start: { on: { F4: 'done' } },
                  done: { type: 'final' }
                }
              }
            },
            onDone: {
              target: 'finished',
              actions: () => {
                doneCount++;
              }
            }
          },
          finished: { type: 'final' }
        }
      });

      const actor = createActor(machine).start();
      expect(actor.getSnapshot().status).toBe('active');

      actor.send({ type: 'F1' });
      expect(actor.getSnapshot().status).toBe('active');
      expect(doneCount).toBe(0);

      actor.send({ type: 'F2' });
      expect(actor.getSnapshot().status).toBe('active');
      expect(doneCount).toBe(0);

      actor.send({ type: 'F3' });
      expect(actor.getSnapshot().status).toBe('active');
      expect(doneCount).toBe(0);

      actor.send({ type: 'F4' });
      expect(actor.getSnapshot().status).toBe('done');
      expect(doneCount).toBe(1);
      expect(actor.getSnapshot().value).toBe('finished');
    });

    it('synchronizes 4 parallel regions completing simultaneously via broadcast event', () => {
      let doneCount = 0;
      const machine = createMachine({
        id: 'par4_simul',
        initial: 'active',
        states: {
          active: {
            type: 'parallel',
            states: {
              r1: {
                initial: 'start',
                states: {
                  start: { on: { FINISH_ALL: 'done' } },
                  done: { type: 'final' }
                }
              },
              r2: {
                initial: 'start',
                states: {
                  start: { on: { FINISH_ALL: 'done' } },
                  done: { type: 'final' }
                }
              },
              r3: {
                initial: 'start',
                states: {
                  start: { on: { FINISH_ALL: 'done' } },
                  done: { type: 'final' }
                }
              },
              r4: {
                initial: 'start',
                states: {
                  start: { on: { FINISH_ALL: 'done' } },
                  done: { type: 'final' }
                }
              }
            },
            onDone: {
              target: 'finished',
              actions: () => {
                doneCount++;
              }
            }
          },
          finished: { type: 'final' }
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'FINISH_ALL' });

      expect(doneCount).toBe(1);
      expect(actor.getSnapshot().value).toBe('finished');
      expect(actor.getSnapshot().status).toBe('done');
    });

    it('synchronizes parallel completion when one region settles immediately via always', () => {
      let doneCount = 0;
      const machine = createMachine({
        id: 'par_always',
        initial: 'active',
        states: {
          active: {
            type: 'parallel',
            states: {
              r1: {
                initial: 'start',
                states: {
                  start: { always: 'done' },
                  done: { type: 'final' }
                }
              },
              r2: {
                initial: 'start',
                states: {
                  start: { on: { FINISH_R2: 'done' } },
                  done: { type: 'final' }
                }
              }
            },
            onDone: {
              target: 'finished',
              actions: () => {
                doneCount++;
              }
            }
          },
          finished: { type: 'final' }
        }
      });

      const actor = createActor(machine).start();
      expect(actor.getSnapshot().value).toEqual({
        active: {
          r1: 'done',
          r2: 'start'
        }
      });

      actor.send({ type: 'FINISH_R2' });
      expect(doneCount).toBe(1);
      expect(actor.getSnapshot().value).toBe('finished');
      expect(actor.getSnapshot().status).toBe('done');
    });
  });

  // =========================================================================
  // 3. History Pseudo-States Inside Parallel Regions
  // =========================================================================
  describe('History Pseudo-States Inside Parallel Regions', () => {
    it('does NOT prevent parallel state completion when regions have history pseudo-states', () => {
      let onDoneFired = false;
      const machine = createMachine({
        id: 'par_hist',
        initial: 'active',
        states: {
          active: {
            type: 'parallel',
            states: {
              hist_parallel: {
                type: 'history'
              },
              r1: {
                initial: 'start',
                states: {
                  hist_r1: { type: 'history' },
                  start: { on: { F1: 'done' } },
                  done: { type: 'final' }
                }
              },
              r2: {
                initial: 'start',
                states: {
                  start: { on: { F2: 'done' } },
                  done: { type: 'final' }
                }
              }
            },
            onDone: {
              target: 'finished',
              actions: () => {
                onDoneFired = true;
              }
            }
          },
          finished: { type: 'final' }
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'F1' });
      expect(onDoneFired).toBe(false);

      actor.send({ type: 'F2' });
      expect(onDoneFired).toBe(true);
      expect(actor.getSnapshot().value).toBe('finished');
      expect(actor.getSnapshot().status).toBe('done');
    });
  });

  // =========================================================================
  // 4. Root Machine Status Invariant
  // =========================================================================
  describe('Root Machine Status Invariant', () => {
    it('keeps root status active when inner compound child reaches final state', () => {
      const exitSpy = vi.fn();
      const machine = createMachine({
        id: 'inner_final_status',
        initial: 'step1',
        exit: exitSpy,
        states: {
          step1: {
            initial: 'inner',
            states: {
              inner: {
                initial: 'working',
                states: {
                  working: { on: { FINISH_INNER: 'inner_done' } },
                  inner_done: { type: 'final' }
                }
              }
            },
            on: {
              NEXT: 'step2'
            }
          },
          step2: {
            on: {
              DONE: 'root_done'
            }
          },
          root_done: {
            type: 'final'
          }
        }
      });

      const actor = createActor(machine).start();
      expect(actor.getSnapshot().status).toBe('active');

      actor.send({ type: 'FINISH_INNER' });

      // When only inner compound child is in final state:
      // 1. Root machine status must be 'active', NOT 'done'
      expect(actor.getSnapshot().status).toBe('active');
      // 2. Machine exit action must NOT have been called yet
      expect(exitSpy).not.toHaveBeenCalled();

      // 3. Machine must still accept subsequent events
      actor.send({ type: 'NEXT' });
      expect(actor.getSnapshot().value).toBe('step2');
      expect(actor.getSnapshot().status).toBe('active');

      // 4. Now transition to root done
      actor.send({ type: 'DONE' });
      expect(actor.getSnapshot().value).toBe('root_done');
      expect(actor.getSnapshot().status).toBe('done');
      expect(exitSpy).toHaveBeenCalledTimes(1);
    });

    it('does NOT wipe active timers when inner compound child reaches final', () => {
      const timeoutSpy = vi.fn();
      const machine = createMachine({
        id: 'timer_preservation',
        initial: 'step1',
        states: {
          step1: {
            initial: 'inner',
            after: {
              50: {
                target: 'timed_out',
                actions: timeoutSpy
              }
            },
            states: {
              inner: {
                initial: 'working',
                states: {
                  working: { on: { FINISH_INNER: 'inner_done' } },
                  inner_done: { type: 'final' }
                }
              }
            }
          },
          timed_out: {
            type: 'final'
          }
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'FINISH_INNER' });

      // Inner is done, but step1 has a 50ms timer!
      // In Rust, handle_machine_completion did: self.active_timers.clear()!
      // If timers were wiped, the after timer will never fire!
      return new Promise<void>((resolve, reject) => {
        setTimeout(() => {
          try {
            expect(actor.getSnapshot().value).toBe('timed_out');
            expect(timeoutSpy).toHaveBeenCalled();
            resolve();
          } catch (e) {
            reject(e);
          }
        }, 120);
      });
    });
  });

  // =========================================================================
  // 5. Exit Actions Order and Root Exit Action Event Passing
  // =========================================================================
  describe('Exit Actions Order and Event Passing', () => {
    it('executes exit actions in reverse document order and passes triggering event to root exit', () => {
      const exitOrder: string[] = [];
      let rootExitEvent: any = null;

      const machine = createMachine({
        id: 'exit_order_machine',
        initial: 'a',
        exit: ({ event }) => {
          exitOrder.push('exit:root');
          rootExitEvent = event;
        },
        states: {
          a: {
            initial: 'a1',
            exit: () => { exitOrder.push('exit:a'); },
            states: {
              a1: {
                initial: 'a1_sub',
                exit: () => { exitOrder.push('exit:a1'); },
                states: {
                  a1_sub: {
                    exit: () => { exitOrder.push('exit:a1_sub'); },
                    on: {
                      FINISH: 'done_state'
                    }
                  }
                }
              }
            }
          },
          done_state: {
            type: 'final'
          }
        }
      });

      const actor = createActor(machine).start();
      exitOrder.length = 0;

      actor.send({ type: 'FINISH', customData: 999 });

      expect(exitOrder).toEqual([
        'exit:a1_sub',
        'exit:a1',
        'exit:a',
        'exit:root'
      ]);
      expect(rootExitEvent).toEqual({
        type: 'FINISH',
        customData: 999
      });
    });
  });

  // =========================================================================
  // 6. Root Re-entry and Literal Hash Target ('#')
  // =========================================================================
  describe('Root Re-entry and Literal Hash Target (#)', () => {
    it('handles target: "#" with reenter: true on machine without explicit ID', () => {
      const trace: string[] = [];

      const machine = createMachine({
        entry: () => { trace.push('entry:root'); },
        exit: () => { trace.push('exit:root'); },
        initial: 'step1',
        states: {
          step1: {
            entry: () => { trace.push('entry:step1'); },
            exit: () => { trace.push('exit:step1'); },
            on: { NEXT: 'step2' }
          },
          step2: {
            entry: () => { trace.push('entry:step2'); },
            exit: () => { trace.push('exit:step2'); },
            on: {
              RESET_HASH: {
                target: '#',
                reenter: true
              }
            }
          }
        }
      });

      const actor = createActor(machine).start();
      expect(actor.getSnapshot().value).toBe('step1');
      expect(trace).toEqual(['entry:root', 'entry:step1']);

      actor.send({ type: 'NEXT' });
      expect(actor.getSnapshot().value).toBe('step2');

      trace.length = 0;
      actor.send({ type: 'RESET_HASH' });

      // Must be back in step1, NOT corrupted '#'
      expect(actor.getSnapshot().value).toBe('step1');
      expect(trace).toEqual([
        'exit:step2',
        'exit:root',
        'entry:root',
        'entry:step1'
      ]);
    });

    it('handles target: "#" with reenter: false (root does not exit or enter)', () => {
      const trace: string[] = [];

      const machine = createMachine({
        entry: () => { trace.push('entry:root'); },
        exit: () => { trace.push('exit:root'); },
        initial: 'step1',
        states: {
          step1: {
            entry: () => { trace.push('entry:step1'); },
            exit: () => { trace.push('exit:step1'); },
            on: { NEXT: 'step2' }
          },
          step2: {
            entry: () => { trace.push('entry:step2'); },
            exit: () => { trace.push('exit:step2'); },
            on: {
              RESET_INTERNAL: {
                target: '#',
                reenter: false
              }
            }
          }
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'NEXT' });
      expect(actor.getSnapshot().value).toBe('step2');

      trace.length = 0;
      actor.send({ type: 'RESET_INTERNAL' });

      expect(actor.getSnapshot().value).toBe('step1');
      // Root must NOT exit or enter! Only step2 exits and step1 enters.
      expect(trace).toEqual(['exit:step2', 'entry:step1']);
    });

    it('handles target: "#" and target: "#customId" on machine with explicit ID', () => {
      const trace: string[] = [];

      const machine = createMachine({
        id: 'checkout_flow',
        entry: () => { trace.push('entry:root'); },
        exit: () => { trace.push('exit:root'); },
        initial: 'cart',
        states: {
          cart: {
            entry: () => { trace.push('entry:cart'); },
            exit: () => { trace.push('exit:cart'); },
            on: { CHECKOUT: 'payment' }
          },
          payment: {
            entry: () => { trace.push('entry:payment'); },
            exit: () => { trace.push('exit:payment'); },
            on: {
              RESET_BY_HASH: {
                target: '#',
                reenter: true
              },
              RESET_BY_ID: {
                target: '#checkout_flow',
                reenter: true
              }
            }
          }
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'CHECKOUT' });
      expect(actor.getSnapshot().value).toBe('payment');

      trace.length = 0;
      actor.send({ type: 'RESET_BY_HASH' });
      expect(actor.getSnapshot().value).toBe('cart');
      expect(trace).toEqual(['exit:payment', 'exit:root', 'entry:root', 'entry:cart']);

      actor.send({ type: 'CHECKOUT' });
      trace.length = 0;
      actor.send({ type: 'RESET_BY_ID' });
      expect(actor.getSnapshot().value).toBe('cart');
      expect(trace).toEqual(['exit:payment', 'exit:root', 'entry:root', 'entry:cart']);
    });

    it('handles target: "#" from inside a parallel state region', () => {
      const trace: string[] = [];

      const machine = createMachine({
        type: 'parallel',
        entry: () => { trace.push('entry:root'); },
        exit: () => { trace.push('exit:root'); },
        states: {
          auth: {
            initial: 'signed_in',
            states: {
              signed_in: {
                entry: () => { trace.push('entry:signed_in'); },
                exit: () => { trace.push('exit:signed_in'); },
                on: { LOGOUT: 'signed_out' }
              },
              signed_out: {
                entry: () => { trace.push('entry:signed_out'); },
                exit: () => { trace.push('exit:signed_out'); },
                on: {
                  REBOOT_SYSTEM: {
                    target: '#',
                    reenter: true
                  }
                }
              }
            }
          },
          ui: {
            initial: 'idle',
            states: {
              idle: {
                entry: () => { trace.push('entry:idle'); },
                exit: () => { trace.push('exit:idle'); },
                on: { BUSY: 'loading' }
              },
              loading: {
                entry: () => { trace.push('entry:loading'); },
                exit: () => { trace.push('exit:loading'); }
              }
            }
          }
        }
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'LOGOUT' });
      actor.send({ type: 'BUSY' });
      expect(actor.getSnapshot().value).toEqual({
        auth: 'signed_out',
        ui: 'loading'
      });

      trace.length = 0;
      actor.send({ type: 'REBOOT_SYSTEM' });

      // Both regions reset to initial
      expect(actor.getSnapshot().value).toEqual({
        auth: 'signed_in',
        ui: 'idle'
      });

      // Both regions must exit, root exits, root enters, both regions enter
      expect(trace).toContain('exit:signed_out');
      expect(trace).toContain('exit:loading');
      expect(trace).toContain('exit:root');
      expect(trace).toContain('entry:root');
      expect(trace).toContain('entry:signed_in');
      expect(trace).toContain('entry:idle');
    });
  });
});

