import { describe, it, expect, vi } from 'vitest';
import {
  createMachine,
  createActor,
  assign,
  sendTo,
  sendParent
} from '../../lavi-wasm/pkg-npm/lavi';

describe('Milestone 3 Iteration 2 Adversarial Stress Suite', () => {
  describe('Defect 1: Multiple sendParent and sendTo Actions in a Single Transition', () => {
    it('1A: preserves CURRENT_ACTOR across multiple sendParent and assign({ self }) in one transition', () => {
      let capturedChildSelfInAssign: any = null;
      let capturedParentEvents: string[] = [];

      const childMachine = createMachine({
        id: 'child1A',
        initial: 'idle',
        context: { childId: '' },
        states: {
          idle: {
            on: {
              TRIGGER: {
                actions: [
                  sendParent({ type: 'FIRST_PARENT_NOTIFICATION', payload: 1 }),
                  assign({
                    childId: ({ self }) => {
                      capturedChildSelfInAssign = self;
                      return self?.id ?? 'NO_SELF';
                    }
                  }),
                  sendParent({ type: 'SECOND_PARENT_NOTIFICATION', payload: 2 })
                ]
              }
            }
          }
        }
      });

      const parentMachine = createMachine({
        id: 'parent1A',
        initial: 'running',
        context: { eventsReceived: [] as string[] },
        states: {
          running: {
            invoke: {
              id: 'myChild',
              src: childMachine
            },
            on: {
              FIRST_PARENT_NOTIFICATION: {
                actions: assign({
                  eventsReceived: ({ context, event }) => [...context.eventsReceived, (event as any).type]
                })
              },
              SECOND_PARENT_NOTIFICATION: {
                actions: assign({
                  eventsReceived: ({ context, event }) => [...context.eventsReceived, (event as any).type]
                })
              }
            }
          }
        }
      });

      const parentActor = createActor(parentMachine).start();
      const childActor = parentActor.getSnapshot().children.myChild;

      expect(childActor).toBeDefined();

      // Trigger the child transition with multiple sendParents and an assign({ self })
      childActor.send({ type: 'TRIGGER' });

      // 1. Verify child self in assign
      expect(capturedChildSelfInAssign).toBeDefined();
      expect(capturedChildSelfInAssign.id).toBe('myChild');
      expect(childActor.getSnapshot().context.childId).toBe('myChild');

      // 2. Verify parent received BOTH notifications without context clobbering
      const parentContext = parentActor.getSnapshot().context;
      expect(parentContext.eventsReceived).toEqual([
        'FIRST_PARENT_NOTIFICATION',
        'SECOND_PARENT_NOTIFICATION'
      ]);
    });

    it('1B: preserves CURRENT_ACTOR through nested reentrant child-to-sibling and child-to-parent transitions', () => {
      let child1Self: string = '';
      let child2Self: string = '';

      const child2Machine = createMachine({
        id: 'child2',
        initial: 'listening',
        context: { greeted: false },
        states: {
          listening: {
            on: {
              GREET_FROM_1: {
                actions: [
                  sendParent({ type: 'CHILD_2_ACK' }),
                  assign({
                    greeted: ({ self }) => {
                      child2Self = self?.id ?? 'NONE';
                      return true;
                    }
                  })
                ]
              }
            }
          }
        }
      });

      const child1Machine = createMachine({
        id: 'child1',
        initial: 'ready',
        context: { selfCaptured: '' },
        states: {
          ready: {
            on: {
              RUN_INTERACTION: {
                actions: [
                  sendTo('child2', { type: 'GREET_FROM_1' }),
                  assign({
                    selfCaptured: ({ self }) => {
                      child1Self = self?.id ?? 'NONE';
                      return self?.id ?? 'NONE';
                    }
                  }),
                  sendParent({ type: 'CHILD_1_DONE' })
                ]
              }
            }
          }
        }
      });

      const parentMachine = createMachine({
        id: 'parent1B',
        initial: 'idle',
        context: { parentEvents: [] as string[] },
        states: {
          idle: {
            invoke: [
              { id: 'child1', src: child1Machine },
              { id: 'child2', src: child2Machine }
            ],
            on: {
              CHILD_2_ACK: {
                actions: assign({
                  parentEvents: ({ context }) => [...context.parentEvents, 'CHILD_2_ACK']
                })
              },
              CHILD_1_DONE: {
                actions: assign({
                  parentEvents: ({ context }) => [...context.parentEvents, 'CHILD_1_DONE']
                })
              }
            }
          }
        }
      });

      const parentActor = createActor(parentMachine).start();
      const c1 = parentActor.getSnapshot().children.child1;
      const c2 = parentActor.getSnapshot().children.child2;

      expect(c1).toBeDefined();
      expect(c2).toBeDefined();

      c1.send({ type: 'RUN_INTERACTION' });

      // Check self identities captured during the nested reentrant transitions
      expect(child1Self).toBe('child1');
      expect(child2Self).toBe('child2');
      expect(c2.getSnapshot().context.greeted).toBe(true);
      expect(c1.getSnapshot().context.selfCaptured).toBe('child1');

      // Parent should have received both CHILD_2_ACK (from child2 during sendTo) and CHILD_1_DONE (from child1)
      expect(parentActor.getSnapshot().context.parentEvents).toContain('CHILD_2_ACK');
      expect(parentActor.getSnapshot().context.parentEvents).toContain('CHILD_1_DONE');
    });

    it('1C: maintains CURRENT_ACTOR across multi-hop ping-pong reentrancy without stack clobbering', () => {
      const callLog: string[] = [];

      const pingMachine = createMachine({
        id: 'pingActor',
        initial: 'active',
        context: { pings: 0 },
        states: {
          active: {
            on: {
              PING: {
                actions: [
                  ({ self }) => { callLog.push(`ping_pre:${self?.id}`); },
                  sendTo('pongActor', { type: 'PONG' }),
                  ({ self }) => { callLog.push(`ping_post:${self?.id}`); },
                  assign({ pings: ({ context }) => context.pings + 1 })
                ]
              }
            }
          }
        }
      });

      const pongMachine = createMachine({
        id: 'pongActor',
        initial: 'active',
        context: { pongs: 0 },
        states: {
          active: {
            on: {
              PONG: {
                actions: [
                  ({ self }) => { callLog.push(`pong_body:${self?.id}`); },
                  assign({ pongs: ({ context }) => context.pongs + 1 })
                ]
              }
            }
          }
        }
      });

      const orchestratorMachine = createMachine({
        id: 'orchestrator',
        initial: 'running',
        states: {
          running: {
            invoke: [
              { id: 'pingActor', src: pingMachine },
              { id: 'pongActor', src: pongMachine }
            ]
          }
        }
      });

      const orch = createActor(orchestratorMachine).start();
      const ping = orch.getSnapshot().children.pingActor;
      const pong = orch.getSnapshot().children.pongActor;

      ping.send({ type: 'PING' });

      expect(callLog).toEqual([
        'ping_pre:pingActor',
        'pong_body:pongActor',
        'ping_post:pingActor'
      ]);
      expect(ping.getSnapshot().context.pings).toBe(1);
      expect(pong.getSnapshot().context.pongs).toBe(1);
    });
  });

  describe('Defect 2: Immediate Child Completion Upon Actor::start_internal()', () => {
    it('2A: delivers xstate.done.actor.<child_id> with output synchronously on start without hanging', () => {
      const childMachine = createMachine({
        id: 'immediateChild',
        initial: 'doneState',
        states: {
          doneState: {
            type: 'final',
            output: { result: 42, text: 'completed_instantly' }
          }
        }
      });

      const parentMachine = createMachine({
        id: 'parent2A',
        initial: 'pending',
        context: { finalResult: null as any },
        states: {
          pending: {
            invoke: {
              id: 'instantWorker',
              src: childMachine,
              onDone: {
                target: 'success',
                actions: assign({
                  finalResult: ({ event }) => (event as any).output
                })
              }
            }
          },
          success: {
            type: 'final'
          }
        }
      });

      const parentActor = createActor(parentMachine).start();

      // Parent must transition directly to 'success' on start() without hanging or requiring a manual event/tick
      expect(parentActor.getSnapshot().value).toBe('success');
      expect(parentActor.getSnapshot().status).toBe('done');
      expect(parentActor.getSnapshot().context.finalResult).toEqual({
        result: 42,
        text: 'completed_instantly'
      });
    });

    it('2B: evaluates dynamic leaf output for immediate child completion on start', () => {
      const dynamicChild = createMachine({
        id: 'dynamicChild',
        initial: 'finalState',
        context: { multiplier: 3, base: 7 },
        states: {
          finalState: {
            type: 'final',
            output: ({ context }) => ({
              product: context.base * context.multiplier
            })
          }
        }
      });

      const parentMachine = createMachine({
        id: 'parent2B',
        initial: 'invoking',
        context: { computed: 0 },
        states: {
          invoking: {
            invoke: {
              id: 'dynCalc',
              src: dynamicChild,
              onDone: {
                target: 'calculated',
                actions: assign({
                  computed: ({ event }) => (event as any).output.product
                })
              }
            }
          },
          calculated: {}
        }
      });

      const actor = createActor(parentMachine).start();
      expect(actor.getSnapshot().value).toBe('calculated');
      expect(actor.getSnapshot().context.computed).toBe(21);
    });

    it('2C: chains multiple sequential immediate child invocations synchronously on start', () => {
      const step1Child = createMachine({
        id: 'step1Child',
        initial: 'done1',
        states: {
          done1: {
            type: 'final',
            output: { step: 1 }
          }
        }
      });

      const step2Child = createMachine({
        id: 'step2Child',
        initial: 'done2',
        states: {
          done2: {
            type: 'final',
            output: { step: 2 }
          }
        }
      });

      const parentMachine = createMachine({
        id: 'parent2C',
        initial: 'first',
        context: { stepsCompleted: [] as number[] },
        states: {
          first: {
            invoke: {
              id: 'childStep1',
              src: step1Child,
              onDone: {
                target: 'second',
                actions: assign({
                  stepsCompleted: ({ context, event }) => [...context.stepsCompleted, (event as any).output.step]
                })
              }
            }
          },
          second: {
            invoke: {
              id: 'childStep2',
              src: step2Child,
              onDone: {
                target: 'allDone',
                actions: assign({
                  stepsCompleted: ({ context, event }) => [...context.stepsCompleted, (event as any).output.step]
                })
              }
            }
          },
          allDone: {
            type: 'final'
          }
        }
      });

      const actor = createActor(parentMachine).start();

      expect(actor.getSnapshot().value).toBe('allDone');
      expect(actor.getSnapshot().status).toBe('done');
      expect(actor.getSnapshot().context.stepsCompleted).toEqual([1, 2]);
    });

    it('2D: handles multiple parallel immediate child invocations on start without deadlock', () => {
      const childA = createMachine({
        initial: 'doneA',
        states: { doneA: { type: 'final', output: 'A' } }
      });
      const childB = createMachine({
        initial: 'doneB',
        states: { doneB: { type: 'final', output: 'B' } }
      });

      const parentMachine = createMachine({
        id: 'parent2D',
        type: 'parallel',
        context: { doneList: [] as string[] },
        states: {
          regionA: {
            initial: 'runningA',
            states: {
              runningA: {
                invoke: {
                  id: 'invA',
                  src: childA,
                  onDone: {
                    target: 'finA',
                    actions: assign({
                      doneList: ({ context, event }) => [...context.doneList, (event as any).output]
                    })
                  }
                }
              },
              finA: { type: 'final' }
            }
          },
          regionB: {
            initial: 'runningB',
            states: {
              runningB: {
                invoke: {
                  id: 'invB',
                  src: childB,
                  onDone: {
                    target: 'finB',
                    actions: assign({
                      doneList: ({ context, event }) => [...context.doneList, (event as any).output]
                    })
                  }
                }
              },
              finB: { type: 'final' }
            }
          }
        }
      });

      const actor = createActor(parentMachine).start();
      expect(actor.getSnapshot().status).toBe('done');
      expect(actor.getSnapshot().context.doneList).toContain('A');
      expect(actor.getSnapshot().context.doneList).toContain('B');
    });
  });

  describe('Defect 6: Microstep Cascade Loops (always) and last_exited_paths Accumulation', () => {
    it('6A: cleanly stops invoked children across multi-hop eventless (always) microstep cascades without leaks', () => {
      const stoppedLogs: string[] = [];

      const makeWorker = (name: string) =>
        createMachine({
          id: name,
          initial: 'working',
          states: {
            working: {}
          }
        });

      const cascadeMachine = createMachine({
        id: 'cascadeMachine',
        initial: 's0',
        states: {
          s0: {
            invoke: { id: 'worker0', src: makeWorker('w0') },
            on: {
              GO: 's1'
            }
          },
          s1: {
            invoke: { id: 'worker1', src: makeWorker('w1') },
            always: 's2'
          },
          s2: {
            invoke: { id: 'worker2', src: makeWorker('w2') },
            always: 's3'
          },
          s3: {
            invoke: { id: 'worker3', src: makeWorker('w3') },
            on: {
              FINISH: 's4'
            }
          },
          s4: {}
        }
      });

      const actor = createActor(cascadeMachine).start();

      // In s0, worker0 should be active
      expect(actor.getSnapshot().value).toBe('s0');
      expect(actor.getSnapshot().children.worker0).toBeDefined();

      // Trigger cascade: s0 -> s1 (always) -> s2 (always) -> s3
      actor.send({ type: 'GO' });

      expect(actor.getSnapshot().value).toBe('s3');

      // worker0, worker1, worker2 must be stopped and cleared from snapshot.children
      const kids = actor.getSnapshot().children;
      expect(kids.worker0).toBeUndefined();
      expect(kids.worker1).toBeUndefined();
      expect(kids.worker2).toBeUndefined();
      expect(kids.worker3).toBeDefined();
    });

    it('6B: handles bounded microstep loops with state re-entry without duplicate stops or cycle deadlock', () => {
      let loopCounter = 0;

      const loopMachine = createMachine({
        id: 'loopMachine',
        initial: 'start',
        context: { count: 0 },
        states: {
          start: {
            on: {
              START_LOOP: 'hopA'
            }
          },
          hopA: {
            always: 'hopB'
          },
          hopB: {
            always: [
              {
                guard: ({ context }) => context.count < 3,
                target: 'hopA',
                actions: assign({
                  count: ({ context }) => {
                    loopCounter++;
                    return context.count + 1;
                  }
                })
              },
              {
                target: 'settled'
              }
            ]
          },
          settled: {
            type: 'final'
          }
        }
      });

      const actor = createActor(loopMachine).start();
      actor.send({ type: 'START_LOOP' });

      expect(actor.getSnapshot().value).toBe('settled');
      expect(actor.getSnapshot().status).toBe('done');
      expect(actor.getSnapshot().context.count).toBe(3);
      expect(loopCounter).toBe(3);
    });

    it('6C: parallel microstep cascades accumulate exited paths across multiple regions simultaneously', () => {
      const parallelCascade = createMachine({
        id: 'parallelCascade',
        type: 'parallel',
        states: {
          reg1: {
            initial: 'r1_start',
            states: {
              r1_start: {
                on: { RUN: 'r1_hop1' }
              },
              r1_hop1: {
                always: 'r1_hop2'
              },
              r1_hop2: {
                always: 'r1_final'
              },
              r1_final: { type: 'final' }
            }
          },
          reg2: {
            initial: 'r2_start',
            states: {
              r2_start: {
                on: { RUN: 'r2_hop1' }
              },
              r2_hop1: {
                always: 'r2_final'
              },
              r2_final: { type: 'final' }
            }
          }
        }
      });

      const actor = createActor(parallelCascade).start();
      expect(actor.getSnapshot().value).toEqual({
        reg1: 'r1_start',
        reg2: 'r2_start'
      });

      actor.send({ type: 'RUN' });

      expect(actor.getSnapshot().value).toEqual({
        reg1: 'r1_final',
        reg2: 'r2_final'
      });
      expect(actor.getSnapshot().status).toBe('done');
    });
  });

  describe('Defect 7: Sibling Actor Messaging via system.get(id)', () => {
    it('7A: allows sibling actors to message each other directly using sendTo(siblingId)', () => {
      let bobReceived: any = null;

      const bobMachine = createMachine({
        id: 'bob',
        initial: 'idle',
        context: { message: '' },
        states: {
          idle: {
            on: {
              HELLO: {
                actions: assign({
                  message: ({ event }) => {
                    bobReceived = (event as any).text;
                    return (event as any).text;
                  }
                })
              }
            }
          }
        }
      });

      const aliceMachine = createMachine({
        id: 'alice',
        initial: 'idle',
        states: {
          idle: {
            on: {
              NOTIFY_BOB: {
                actions: sendTo('bob', { type: 'HELLO', text: 'Hey Bob!' })
              }
            }
          }
        }
      });

      const familyMachine = createMachine({
        id: 'family',
        initial: 'active',
        states: {
          active: {
            invoke: [
              { id: 'alice', src: aliceMachine },
              { id: 'bob', src: bobMachine }
            ]
          }
        }
      });

      const familyActor = createActor(familyMachine).start();
      const alice = familyActor.getSnapshot().children.alice;
      const bob = familyActor.getSnapshot().children.bob;

      expect(alice).toBeDefined();
      expect(bob).toBeDefined();

      // Alice sends a message to Bob via sendTo('bob', ...)
      alice.send({ type: 'NOTIFY_BOB' });

      expect(bobReceived).toBe('Hey Bob!');
      expect(bob.getSnapshot().context.message).toBe('Hey Bob!');
    });

    it('7B: supports bidirectional conversational ping-pong between siblings', () => {
      const transcript: string[] = [];

      const serverMachine = createMachine({
        id: 'serverActor',
        initial: 'listening',
        states: {
          listening: {
            on: {
              REQ: {
                actions: [
                  ({ event }) => transcript.push(`server:recv:${(event as any).data}`),
                  sendTo('clientActor', { type: 'RES', data: 'pong_data' })
                ]
              }
            }
          }
        }
      });

      const clientMachine = createMachine({
        id: 'clientActor',
        initial: 'init',
        context: { gotResponse: false },
        states: {
          init: {
            on: {
              CALL_SERVER: {
                actions: [
                  ({ event }) => transcript.push(`client:send:${(event as any).data}`),
                  sendTo('serverActor', ({ event }) => ({ type: 'REQ', data: (event as any).data }))
                ]
              },
              RES: {
                actions: [
                  ({ event }) => transcript.push(`client:recv:${(event as any).data}`),
                  assign({ gotResponse: true })
                ]
              }
            }
          }
        }
      });

      const rootMachine = createMachine({
        id: 'rootMesh',
        initial: 'active',
        states: {
          active: {
            invoke: [
              { id: 'clientActor', src: clientMachine },
              { id: 'serverActor', src: serverMachine }
            ]
          }
        }
      });

      const root = createActor(rootMachine).start();
      const client = root.getSnapshot().children.clientActor;
      const server = root.getSnapshot().children.serverActor;

      expect(client).toBeDefined();
      expect(server).toBeDefined();

      client.send({ type: 'CALL_SERVER', data: 'hello_server' });

      expect(transcript).toEqual([
        'client:send:hello_server',
        'server:recv:hello_server',
        'client:recv:pong_data'
      ]);
      expect(client.getSnapshot().context.gotResponse).toBe(true);
    });

    it('7C: routes messages around a 3-sibling ring (A -> B -> C -> A)', () => {
      const ringHops: string[] = [];

      const makeNode = (id: string, nextId: string) =>
        createMachine({
          id,
          initial: 'idle',
          states: {
            idle: {
              on: {
                TOKEN: {
                  actions: [
                    ({ event }) => ringHops.push(`${id}:${(event as any).count}`),
                    sendTo(nextId, ({ event }) => ({
                      type: 'TOKEN',
                      count: (event as any).count + 1
                    }))
                  ],
                  target: 'passed'
                }
              },
              passed: {}
            }
          }
        });

      const ringMachine = createMachine({
        id: 'ring',
        initial: 'active',
        states: {
          active: {
            invoke: [
              { id: 'nodeA', src: makeNode('nodeA', 'nodeB') },
              { id: 'nodeB', src: makeNode('nodeB', 'nodeC') },
              { id: 'nodeC', src: makeNode('nodeC', 'nodeA') }
            ]
          }
        }
      });

      const ringActor = createActor(ringMachine).start();
      const nodeA = ringActor.getSnapshot().children.nodeA;

      // Inject token at nodeA
      nodeA.send({ type: 'TOKEN', count: 1 });

      expect(ringHops).toEqual([
        'nodeA:1',
        'nodeB:2',
        'nodeC:3'
      ]);
    });

    it('7D: safely ignores sendTo with unknown target ID without crashing or halting execution', () => {
      let continuedActionExecuted = false;

      const testMachine = createMachine({
        id: 'safeSender',
        initial: 'active',
        states: {
          active: {
            on: {
              SEND_UNKNOWN: {
                actions: [
                  sendTo('ghostActor', { type: 'PHANTOM_EVENT' }),
                  () => {
                    continuedActionExecuted = true;
                  }
                ]
              }
            }
          }
        }
      });

      const actor = createActor(testMachine).start();

      expect(() => {
        actor.send({ type: 'SEND_UNKNOWN' });
      }).not.toThrow();

      expect(continuedActionExecuted).toBe(true);
    });
  });
});
