const { createMachine, createActor, setup } = require('../portable-xstate-wasm/pkg-npm/portable_xstate_wasm.js');

function trackEntries(machine) {
  let logs = [];
  function injectTrackers(stateConf, path) {
    if (stateConf.entry === undefined) stateConf.entry = [];
    if (!Array.isArray(stateConf.entry)) stateConf.entry = [stateConf.entry];
    stateConf.entry.unshift(() => {
      logs.push(`enter: ${path}`);
    });

    if (stateConf.exit === undefined) stateConf.exit = [];
    if (!Array.isArray(stateConf.exit)) stateConf.exit = [stateConf.exit];
    stateConf.exit.unshift(() => {
      logs.push(`exit: ${path}`);
    });

    if (stateConf.states) {
      for (const [key, val] of Object.entries(stateConf.states)) {
        injectTrackers(val, path === '__root__' ? key : `${path}.${key}`);
      }
    }
  }

  const config = machine.config;
  if (config) {
    injectTrackers(config, '__root__');
  }

  return () => {
    const flushed = logs;
    logs = [];
    return flushed;
  };
}

const machine = createMachine({
  initial: 'a',
  states: {
    a: {
      initial: 'a1',
      states: {
        a1: {
          on: {
            NEXT_FN: 'a3'
          }
        },
        a2: {},
        a3: {
          on: {
            NEXT: {
              target: 'a2'
            }
          }
        }
      }
    }
  }
});

const flushTracked = trackEntries(machine);

const actor = createActor(machine).start();
console.log("ON START:", flushTracked());

actor.send({ type: 'NEXT_FN' });

console.log("AFTER NEXT_FN:", flushTracked());
