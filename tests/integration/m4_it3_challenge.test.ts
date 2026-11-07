import { describe, it, expect, vi } from 'vitest';
import {
  createMachine,
  createActor,
  setup,
  assign,
  and,
  not,
  transition,
  initialTransition
} from '../../lavi-wasm/pkg-npm/lavi';

describe('Milestone 4 Iteration 3 Pure Stepping & Parallel Conformance Challenge', () => {
  // =========================================================================
  // Dimension 1: 1-Region Parallel Machines in Pure Stepping
  // =========================================================================
  describe('Dimension 1: 1-Region Parallel Machine Pure Stepping', () => {
    it('1.1 resolves initial state and transitions properly for a single-region parallel machine', () => {
      const entrySpy = vi.fn();
      const actionSpy = vi.fn();

      const machine = createMachine({
        id: 'single_region_parallel',
        type: 'parallel',
        context: { counter: 0 },
        states: {
          single: {
            initial: 'idle',
            entry: entrySpy,
            states: {
              idle: {
                on: {
                  STEP: {
                    target: 'done',
                    actions: [
                      actionSpy,
                      assign({ counter: ({ context }: any) => context.counter + 1 }),
                      { type: 'onSingleStep', params: { detail: 'step1' } }
                    ]
                  }
                }
              },
              done: {}
            }
          }
        }
      });

      // 1. Initial transition
      const [s0, a0] = initialTransition(machine);
      expect(s0.value).toEqual({ single: 'idle' });
      expect(s0.context).toEqual({ counter: 0 });
      expect(entrySpy).not.toHaveBeenCalled();

      // 2. Pure transition
      const [s1, a1] = transition(machine, s0, { type: 'STEP' });
      expect(s1.value).toEqual({ single: 'done' });
      expect(s1.context).toEqual({ counter: 1 });
      expect(actionSpy).not.toHaveBeenCalled();
      expect(a1).toContainEqual(
        expect.objectContaining({
          type: 'onSingleStep',
          params: { detail: 'step1' }
        })
      );

      // 3. Parity with createActor
      const actor = createActor(machine).start();
      actor.send({ type: 'STEP' });
      const snap = actor.getSnapshot();
      expect(s1.value).toEqual(snap.value);
      expect(s1.context).toEqual(snap.context);
    });

    it('1.2 preserves parallel structure when re-stepping from existing snapshot', () => {
      const machine = createMachine({
        id: 'single_region_re_step',
        type: 'parallel',
        states: {
          region1: {
            initial: 'alpha',
            states: {
              alpha: {
                on: { NEXT: 'beta' }
              },
              beta: {
                on: { NEXT: 'gamma' }
              },
              gamma: {}
            }
          }
        }
      });

      const [s0] = initialTransition(machine);
      expect(s0.value).toEqual({ region1: 'alpha' });

      const [s1] = transition(machine, s0, { type: 'NEXT' });
      expect(s1.value).toEqual({ region1: 'beta' });

      // Unhandled event should keep region1 in beta without corrupting the value
      const [s1b] = transition(machine, s1, { type: 'UNHANDLED' });
      expect(s1b.value).toEqual({ region1: 'beta' });

      const [s2] = transition(machine, s1b, { type: 'NEXT' });
      expect(s2.value).toEqual({ region1: 'gamma' });
    });
  });

  // =========================================================================
  // Dimension 2: 2-Region Parallel Machines (Simultaneous & Orthogonal Events)
  // =========================================================================
  describe('Dimension 2: 2-Region Parallel Machine Pure Stepping', () => {
    it('2.1 executes simultaneous events across both regions with pure assigns and action capture', () => {
      const aSpy = vi.fn();
      const bSpy = vi.fn();

      const machine = createMachine({
        id: 'two_region_simultaneous',
        type: 'parallel',
        context: { aCount: 0, bCount: 0 },
        states: {
          regA: {
            initial: 'a0',
            states: {
              a0: {
                on: {
                  SYNC: {
                    target: 'a1',
                    actions: [
                      aSpy,
                      assign({ aCount: ({ context }: any) => context.aCount + 10 }),
                      { type: 'onRegA', params: ({ context }: any) => ({ a: context.aCount }) }
                    ]
                  }
                }
              },
              a1: {}
            }
          },
          regB: {
            initial: 'b0',
            states: {
              b0: {
                on: {
                  SYNC: {
                    target: 'b1',
                    actions: [
                      bSpy,
                      assign({ bCount: ({ context }: any) => context.bCount + 20 }),
                      { type: 'onRegB', params: ({ context }: any) => ({ b: context.bCount }) }
                    ]
                  }
                }
              },
              b1: {}
            }
          }
        }
      });

      const [s0] = initialTransition(machine);
      expect(s0.value).toEqual({ regA: 'a0', regB: 'b0' });
      expect(s0.context).toEqual({ aCount: 0, bCount: 0 });

      // Simultaneous SYNC event transitions both regA and regB
      const [s1, a1] = transition(machine, s0, { type: 'SYNC' });
      expect(s1.value).toEqual({ regA: 'a1', regB: 'b1' });
      expect(s1.context).toEqual({ aCount: 10, bCount: 20 });

      // Purity check: Spies must not be invoked
      expect(aSpy).not.toHaveBeenCalled();
      expect(bSpy).not.toHaveBeenCalled();

      // Action descriptors must be captured for both regions
      expect(a1).toContainEqual(
        expect.objectContaining({
          type: 'onRegA',
          params: { a: 10 }
        })
      );
      expect(a1).toContainEqual(
        expect.objectContaining({
          type: 'onRegB',
          params: { b: 20 }
        })
      );

      // Parity with createActor
      const actor = createActor(machine).start();
      actor.send({ type: 'SYNC' });
      const snap = actor.getSnapshot();
      expect(s1.value).toEqual(snap.value);
      expect(s1.context).toEqual(snap.context);
    });

    it('2.2 handles orthogonal transitions affecting one region while keeping the other stable', () => {
      const machine = createMachine({
        id: 'two_region_orthogonal',
        type: 'parallel',
        context: { regAState: 'off', regBState: 'inactive' },
        states: {
          regA: {
            initial: 'off',
            states: {
              off: {
                on: { TOGGLE_A: { target: 'on', actions: assign({ regAState: 'on' }) } }
              },
              on: {
                on: { TOGGLE_A: { target: 'off', actions: assign({ regAState: 'off' }) } }
              }
            }
          },
          regB: {
            initial: 'inactive',
            states: {
              inactive: {
                on: { TOGGLE_B: { target: 'active', actions: assign({ regBState: 'active' }) } }
              },
              active: {
                on: { TOGGLE_B: { target: 'inactive', actions: assign({ regBState: 'inactive' }) } }
              }
            }
          }
        }
      });

      const [s0] = initialTransition(machine);
      expect(s0.value).toEqual({ regA: 'off', regB: 'inactive' });

      // Toggle A only
      const [s1] = transition(machine, s0, { type: 'TOGGLE_A' });
      expect(s1.value).toEqual({ regA: 'on', regB: 'inactive' });
      expect(s1.context).toEqual({ regAState: 'on', regBState: 'inactive' });

      // Toggle B only from s1
      const [s2] = transition(machine, s1, { type: 'TOGGLE_B' });
      expect(s2.value).toEqual({ regA: 'on', regB: 'active' });
      expect(s2.context).toEqual({ regAState: 'on', regBState: 'active' });

      // Toggle A again
      const [s3] = transition(machine, s2, { type: 'TOGGLE_A' });
      expect(s3.value).toEqual({ regA: 'off', regB: 'active' });
      expect(s3.context).toEqual({ regAState: 'off', regBState: 'active' });
    });
  });

  // =========================================================================
  // Dimension 3: 3-Region Parallel Machines in Pure Stepping
  // =========================================================================
  describe('Dimension 3: 3-Region Parallel Machine Pure Stepping', () => {
    it('3.1 initial transition resolves all 3 orthogonal regions simultaneously', () => {
      const machine = createMachine({
        id: 'tri_parallel',
        type: 'parallel',
        tags: ['rootTag'],
        states: {
          sensor: {
            initial: 'calibrating',
            tags: ['sensorTag'],
            states: {
              calibrating: { tags: ['calibTag'] },
              online: {}
            }
          },
          actuator: {
            initial: 'disarmed',
            tags: ['actuatorTag'],
            states: {
              disarmed: { tags: ['disarmedTag'] },
              armed: {}
            }
          },
          display: {
            initial: 'splash',
            tags: ['displayTag'],
            states: {
              splash: { tags: ['splashTag'] },
              ready: {}
            }
          }
        }
      });

      const [s0] = initialTransition(machine);
      expect(s0.value).toEqual({
        sensor: 'calibrating',
        actuator: 'disarmed',
        display: 'splash'
      });

      // Tags must be aggregated across all 3 active regions
      expect(s0.hasTag('rootTag')).toBe(true);
      expect(s0.hasTag('sensorTag')).toBe(true);
      expect(s0.hasTag('calibTag')).toBe(true);
      expect(s0.hasTag('actuatorTag')).toBe(true);
      expect(s0.hasTag('disarmedTag')).toBe(true);
      expect(s0.hasTag('displayTag')).toBe(true);
      expect(s0.hasTag('splashTag')).toBe(true);
      expect(s0.hasTag('online')).toBe(false);

      // Matches checks
      expect(s0.matches({ sensor: 'calibrating' })).toBe(true);
      expect(s0.matches({ actuator: 'disarmed' })).toBe(true);
      expect(s0.matches({ display: 'splash' })).toBe(true);
    });

    it('3.2 simultaneous event across 3 regions advances all 3 regions with chained assigns', () => {
      const machine = createMachine({
        id: 'tri_simultaneous',
        type: 'parallel',
        context: { sVal: 0, aVal: 0, dVal: 0, total: 0 },
        states: {
          sensor: {
            initial: 'offline',
            states: {
              offline: {
                on: {
                  BOOT: {
                    target: 'online',
                    actions: assign({ sVal: 1 })
                  }
                }
              },
              online: {}
            }
          },
          actuator: {
            initial: 'offline',
            states: {
              offline: {
                on: {
                  BOOT: {
                    target: 'online',
                    actions: assign({ aVal: 10 })
                  }
                }
              },
              online: {}
            }
          },
          display: {
            initial: 'offline',
            states: {
              offline: {
                on: {
                  BOOT: {
                    target: 'online',
                    actions: assign({ dVal: 100 })
                  }
                }
              },
              online: {}
            }
          }
        }
      });

      const [s0] = initialTransition(machine);
      expect(s0.value).toEqual({
        sensor: 'offline',
        actuator: 'offline',
        display: 'offline'
      });

      const [s1] = transition(machine, s0, { type: 'BOOT' });
      expect(s1.value).toEqual({
        sensor: 'online',
        actuator: 'online',
        display: 'online'
      });
      expect(s1.context).toEqual({ sVal: 1, aVal: 10, dVal: 100, total: 0 });

      // Actor parity check
      const actor = createActor(machine).start();
      actor.send({ type: 'BOOT' });
      const snap = actor.getSnapshot();
      expect(s1.value).toEqual(snap.value);
      expect(s1.context).toEqual(snap.context);
    });
  });

  // =========================================================================
  // Dimension 4: Deep Nested Parallel Machines
  // =========================================================================
  describe('Dimension 4: Deep Nested Parallel Machines in Pure Stepping', () => {
    it('4.1 resolves deep hierarchical parallel state and executes simultaneous child transitions', () => {
      const machine = createMachine({
        id: 'deep_parallel',
        initial: 'system',
        states: {
          system: {
            initial: 'operational',
            states: {
              operational: {
                type: 'parallel',
                states: {
                  region1: {
                    initial: 'sub1_a',
                    states: {
                      sub1_a: {
                        on: { ADVANCE_ALL: 'sub1_b' }
                      },
                      sub1_b: {}
                    }
                  },
                  region2: {
                    initial: 'sub2_a',
                    states: {
                      sub2_a: {
                        on: { ADVANCE_ALL: 'sub2_b' }
                      },
                      sub2_b: {}
                    }
                  },
                  region3: {
                    initial: 'sub3_a',
                    states: {
                      sub3_a: {
                        on: { ADVANCE_ALL: 'sub3_b' }
                      },
                      sub3_b: {}
                    }
                  }
                }
              }
            }
          }
        }
      });

      // 1. Initial transition deep resolution
      const [s0] = initialTransition(machine);
      expect(s0.value).toEqual({
        system: {
          operational: {
            region1: 'sub1_a',
            region2: 'sub2_a',
            region3: 'sub3_a'
          }
        }
      });

      // 2. Simultaneous transition deep in hierarchy
      const [s1] = transition(machine, s0, { type: 'ADVANCE_ALL' });
      expect(s1.value).toEqual({
        system: {
          operational: {
            region1: 'sub1_b',
            region2: 'sub2_b',
            region3: 'sub3_b'
          }
        }
      });

      // 3. Parity with createActor
      const actor = createActor(machine).start();
      actor.send({ type: 'ADVANCE_ALL' });
      const snap = actor.getSnapshot();
      expect(s1.value).toEqual(snap.value);
    });

    it('4.2 allows exiting a deep nested parallel state back to root atomic state', () => {
      const machine = createMachine({
        id: 'exit_deep_parallel',
        initial: 'active',
        context: { terminated: false },
        states: {
          active: {
            type: 'parallel',
            on: {
              ABORT: {
                target: 'shutdown',
                actions: assign({ terminated: true })
              }
            },
            states: {
              r1: {
                initial: 'busy',
                states: { busy: {} }
              },
              r2: {
                initial: 'working',
                states: { working: {} }
              }
            }
          },
          shutdown: {}
        }
      });

      const [s0] = initialTransition(machine);
      expect(s0.value).toEqual({
        active: {
          r1: 'busy',
          r2: 'working'
        }
      });

      const [s1] = transition(machine, s0, { type: 'ABORT' });
      expect(s1.value).toBe('shutdown');
      expect(s1.context.terminated).toBe(true);
    });
  });

  // =========================================================================
  // Dimension 5: Parallel Machines with Guard AST & Conditional Execution
  // =========================================================================
  describe('Dimension 5: Guard AST in Parallel Regions', () => {
    it('5.1 selectively fires transitions in parallel regions based on payload and context guards', () => {
      const machine = createMachine({
        id: 'parallel_guards',
        type: 'parallel',
        context: { threshold: 5 },
        states: {
          regA: {
            initial: 'low',
            states: {
              low: {
                on: {
                  EVAL: {
                    guard: ({ event }: any) => event.val > 5,
                    target: 'high'
                  }
                }
              },
              high: {}
            }
          },
          regB: {
            initial: 'normal',
            states: {
              normal: {
                on: {
                  EVAL: {
                    guard: ({ context, event }: any) => event.val <= context.threshold,
                    target: 'suppressed'
                  }
                }
              },
              suppressed: {}
            }
          }
        }
      });

      const [s0] = initialTransition(machine);
      expect(s0.value).toEqual({ regA: 'low', regB: 'normal' });

      // Event with val = 10: regA transitions to high; regB remains normal
      const [s1] = transition(machine, s0, { type: 'EVAL', val: 10 });
      expect(s1.value).toEqual({ regA: 'high', regB: 'normal' });

      // Event with val = 3 from s0: regA remains low; regB transitions to suppressed
      const [s2] = transition(machine, s0, { type: 'EVAL', val: 3 });
      expect(s2.value).toEqual({ regA: 'low', regB: 'suppressed' });
    });
  });

  // =========================================================================
  // Dimension 6: History State in Parallel Regions & Pure Stepping
  // =========================================================================
  describe('Dimension 6: History State in Parallel Regions', () => {
    it('6.1 restores prior parallel sub-states via history state in pure stepping', () => {
      const machine = createMachine({
        id: 'parallel_history_challenge',
        initial: 'operating',
        states: {
          operating: {
            initial: 'mode',
            states: {
              mode: {
                type: 'parallel',
                states: {
                  light: {
                    initial: 'bright',
                    states: {
                      bright: { on: { DIM: 'dimmed' } },
                      dimmed: {}
                    }
                  },
                  sound: {
                    initial: 'beep',
                    states: {
                      beep: { on: { MUTE: 'silent' } },
                      silent: {}
                    }
                  }
                }
              },
              hist: {
                type: 'history',
                history: 'deep'
              }
            },
            on: { PAUSE: 'paused' }
          },
          paused: {
            on: { RESUME: 'operating.hist' }
          }
        }
      });

      // 1. Actor baseline
      const actor = createActor(machine).start();
      actor.send({ type: 'DIM' });
      actor.send({ type: 'MUTE' });
      actor.send({ type: 'PAUSE' });
      actor.send({ type: 'RESUME' });
      const actorSnap = actor.getSnapshot();
      expect(actorSnap.value).toEqual({
        operating: {
          mode: {
            light: 'dimmed',
            sound: 'silent'
          }
        }
      });

      // 2. Pure transition stepping
      const [s0] = initialTransition(machine);
      expect(s0.value).toEqual({
        operating: {
          mode: {
            light: 'bright',
            sound: 'beep'
          }
        }
      });

      const [s1] = transition(machine, s0, { type: 'DIM' });
      expect(s1.value).toEqual({
        operating: {
          mode: {
            light: 'dimmed',
            sound: 'beep'
          }
        }
      });

      const [s2] = transition(machine, s1, { type: 'MUTE' });
      expect(s2.value).toEqual({
        operating: {
          mode: {
            light: 'dimmed',
            sound: 'silent'
          }
        }
      });

      const [s3] = transition(machine, s2, { type: 'PAUSE' });
      expect(s3.value).toBe('paused');

      const [s4] = transition(machine, s3, { type: 'RESUME' });
      expect(s4.value).toEqual({
        operating: {
          mode: {
            light: 'dimmed',
            sound: 'silent'
          }
        }
      });
      expect(s4.value).toEqual(actorSnap.value);
    });

    it('6.2 4-region parallel machine with simultaneous quad event in pure stepping', () => {
      const machine = createMachine({
        id: 'quad_parallel',
        type: 'parallel',
        context: { r1: 0, r2: 0, r3: 0, r4: 0 },
        states: {
          r1: {
            initial: 'off',
            states: {
              off: { on: { QUAD_PULSE: { target: 'on', actions: assign({ r1: 1 }) } } },
              on: {}
            }
          },
          r2: {
            initial: 'off',
            states: {
              off: { on: { QUAD_PULSE: { target: 'on', actions: assign({ r2: 2 }) } } },
              on: {}
            }
          },
          r3: {
            initial: 'off',
            states: {
              off: { on: { QUAD_PULSE: { target: 'on', actions: assign({ r3: 3 }) } } },
              on: {}
            }
          },
          r4: {
            initial: 'off',
            states: {
              off: { on: { QUAD_PULSE: { target: 'on', actions: assign({ r4: 4 }) } } },
              on: {}
            }
          }
        }
      });

      const [s0] = initialTransition(machine);
      expect(s0.value).toEqual({ r1: 'off', r2: 'off', r3: 'off', r4: 'off' });

      const [s1] = transition(machine, s0, { type: 'QUAD_PULSE' });
      expect(s1.value).toEqual({ r1: 'on', r2: 'on', r3: 'on', r4: 'on' });
      expect(s1.context).toEqual({ r1: 1, r2: 2, r3: 3, r4: 4 });

      // Actor parity
      const actor = createActor(machine).start();
      actor.send({ type: 'QUAD_PULSE' });
      const snap = actor.getSnapshot();
      expect(s1.value).toEqual(snap.value);
      expect(s1.context).toEqual(snap.context);
    });

    it('6.3 evaluates higher-order guards across parallel regions in pure stepping', () => {
      const machine = setup({
        guards: {
          isReady: ({ context }: any) => context.ready === true,
          hasTokens: ({ context }: any) => context.tokens > 0
        }
      }).createMachine({
        id: 'parallel_ho_guards',
        type: 'parallel',
        context: { ready: true, tokens: 5, branchAOk: false, branchBOk: false },
        states: {
          branchA: {
            initial: 'waiting',
            states: {
              waiting: {
                on: {
                  FIRE: {
                    guard: and(['isReady', 'hasTokens']),
                    target: 'fired',
                    actions: [assign({ branchAOk: true })]
                  }
                }
              },
              fired: {}
            }
          },
          branchB: {
            initial: 'waiting',
            states: {
              waiting: {
                on: {
                  FIRE: {
                    guard: not('isReady'),
                    target: 'blocked',
                    actions: assign({ branchBOk: false })
                  }
                }
              },
              blocked: {}
            }
          }
        }
      });

      const [s0] = initialTransition(machine);
      expect(s0.value).toEqual({ branchA: 'waiting', branchB: 'waiting' });

      const [s1] = transition(machine, s0, { type: 'FIRE' });
      // BranchA should fire because isReady && hasTokens is true
      // BranchB should NOT fire because !isReady is false
      expect(s1.value).toEqual({ branchA: 'fired', branchB: 'waiting' });
      expect(s1.context.branchAOk).toBe(true);
      expect(s1.context.branchBOk).toBe(false);
    });
  });

  // =========================================================================
  // Dimension 7: Boundary Conditions & Edge Cases
  // =========================================================================
  describe('Dimension 7: Boundary Conditions & Edge Cases', () => {
    it('7.1 handles empty parallel state safely', () => {
      const machine = createMachine({
        id: 'empty_parallel',
        type: 'parallel',
        states: {}
      });

      const [s0] = initialTransition(machine);
      expect(s0.value).toEqual({});

      const [s1] = transition(machine, s0, { type: 'ANY' });
      expect(s1.value).toEqual({});
    });

    it('7.2 maintains active status until all parallel regions reach final', () => {
      const machine = createMachine({
        id: 'parallel_final_status',
        type: 'parallel',
        states: {
          branchA: {
            initial: 'step1',
            states: {
              step1: { on: { FINISH_A: 'done' } },
              done: { type: 'final' }
            }
          },
          branchB: {
            initial: 'step1',
            states: {
              step1: { on: { FINISH_B: 'done' } },
              done: { type: 'final' }
            }
          }
        }
      });

      const [s0] = initialTransition(machine);
      expect(s0.status).toBe('active');

      // Finish branchA only
      const [s1] = transition(machine, s0, { type: 'FINISH_A' });
      expect(s1.value).toEqual({ branchA: 'done', branchB: 'step1' });
      expect(s1.status).toBe('active');

      // Finish branchB from s1 -> now entire machine is done
      const [s2] = transition(machine, s1, { type: 'FINISH_B' });
      expect(s2.value).toEqual({ branchA: 'done', branchB: 'done' });
      expect(s2.status).toBe('done');
    });
  });
});
