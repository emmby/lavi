#include "../include/lavi.hpp"
#include <iostream>
#include <cassert>

static unsigned long g_fake_millis_ul = 0;
static unsigned long fake_arduino_millis() {
    return g_fake_millis_ul;
}
static uint64_t raw_c_clock_fn() {
    return static_cast<uint64_t>(g_fake_millis_ul);
}

const char* STOPLIGHT_JSON = R"({
  "id": "trafficLight",
  "initial": "green",
  "states": {
    "green": { "on": { "TIMER": [ { "target": "yellow", "actions": ["doTimerAction"] } ] } },
    "yellow": { "on": { "TIMER": [ { "target": "red" } ] } },
    "red": { "on": { "TIMER": [ { "target": "green" } ] } }
  },
  "context": { "cycles": 0 }
})";

int main() {
    std::cout << "Starting C++ Integration Test..." << std::endl;

    try {
        lavi::Machine machine(STOPLIGHT_JSON);

        bool cpp_action_called = false;
        machine.registerAction("doTimerAction", [&cpp_action_called](const lavi::Value& context, const lavi::Value& event) {
            std::cout << "C++ Action called!" << std::endl;
            auto cycles = context.getProperty("cycles");
            assert(!cycles.isNull());
            double cycles_val = -1;
            assert(cycles.getNumber(cycles_val) && cycles_val == 0.0);
            
            auto missing = context.getProperty("missing_key");
            assert(missing.isNull());

            auto ev_type = event.getProperty("type");
            assert(!ev_type.isNull());
            std::string ev_name;
            assert(ev_type.getString(ev_name) && ev_name == "TIMER");

            cpp_action_called = true;
        });

        // Unstarted machine matches nothing
        assert(!machine.matches("green"));

        machine.start();

        assert(machine.matches("green"));
        assert(machine.matches(std::string("green")));
        assert(!machine.matches("yellow"));

        machine.send("TIMER");
        assert(machine.matches("yellow"));
        assert(!machine.matches("green"));
        assert(cpp_action_called == true);

        // Test lavi::SimulatedClock with hierarchical polling, dynamic guards, cascading timers, and std::move
        const char* APRIL_CPP_JSON = R"({
          "id": "aprilCpp",
          "initial": "ConnectToNetwork",
          "states": {
            "ConnectToNetwork": {
              "initial": "Polling",
              "after": { "300000": "Timeout" },
              "states": {
                "Polling": {
                  "entry": ["pollWifi"],
                  "after": {
                    "500": [
                      { "target": "#Wait", "guard": "isConnected" },
                      { "target": "Polling", "reenter": true }
                    ]
                  }
                }
              }
            },
            "Wait": {
              "id": "Wait",
              "after": { "60000": "Sleep" }
            },
            "Sleep": {
              "after": { "3600000": "Wake" }
            },
            "Wake": {},
            "Timeout": {}
          }
        })";

        lavi::SimulatedClock sim_clock(0);
        lavi::Machine april_fsm(APRIL_CPP_JSON, sim_clock);

        int poll_count = 0;
        bool wifi_ready = false;
        uint64_t tenth_poll_timestamp = 0;

        april_fsm.registerGuard("isConnected", [&](const lavi::Value&, const lavi::Value&) {
            return wifi_ready;
        });
        april_fsm.registerAction("pollWifi", [&](const lavi::Value&, const lavi::Value&) {
            poll_count++;
            if (poll_count == 10) {
                wifi_ready = true;
                tenth_poll_timestamp = sim_clock.now();
            }
        });

        april_fsm.start();
        assert(april_fsm.matches("ConnectToNetwork.Polling"));
        assert(poll_count == 1);

        uint64_t next_dl = 0;
        assert(april_fsm.getNextDeadline(next_dl) && next_dl == 500);

        // Move lavi::Machine before increment, and then move SimulatedClock before set
        lavi::Machine moved_fsm = std::move(april_fsm);

        // Advance by 60,000 ms:
        // - 10th poll happens at t = 4,500 ms -> sets wifi_ready = true
        // - At t = 5,000 ms -> transitions to Wait (arming Wait's 60,000ms timer for t = 65,000 ms)
        // - Settles at t = 60,000 ms in Wait
        sim_clock.increment(60000);
        assert(sim_clock.now() == 60000);
        assert(poll_count == 10);
        assert(tenth_poll_timestamp == 4500);
        assert(moved_fsm.matches("Wait"));
        assert(moved_fsm.getNextDeadline(next_dl) && next_dl == 65000);

        // Move SimulatedClock and advance to 86,460,000 ms -> Wait -> Sleep at 65,000 ms, Sleep -> Wake at 3,665,000 ms
        lavi::SimulatedClock moved_clock = std::move(sim_clock);
        moved_clock.set(86460000);
        assert(moved_clock.now() == 86460000);
        assert(moved_fsm.matches("Wake"));
        assert(!moved_fsm.getNextDeadline(next_dl));

        const char* PARALLEL_JSON = R"({
          "id": "displayMachine",
          "initial": "Display",
          "states": {
            "Display": {
              "type": "parallel",
              "states": {
                "Theme": {
                  "initial": "Light",
                  "states": { "Light": {}, "Dark": {} }
                },
                "Power": {
                  "initial": "On",
                  "states": { "On": {}, "Off": {} }
                }
              }
            }
          }
        })";
        lavi::Machine parallelMachine(PARALLEL_JSON);
        parallelMachine.start();
        assert(parallelMachine.matches("Display"));
        assert(parallelMachine.matches("Display.Theme"));
        assert(parallelMachine.matches("Display.Theme.Light"));
        assert(parallelMachine.matches("Display.Power.On"));
        assert(!parallelMachine.matches("Display.Power.Off"));

        // Wildcard and partial wildcard transition test with guarded fallback
        const char* WILDCARD_CPP_JSON = R"({
          "initial": "A",
          "states": {
            "A": {
              "on": {
                "*": "catchAll",
                "foo.*": "shortPartial",
                "foo.bar.*": {
                  "target": "longPartial",
                  "guard": "allowLong"
                },
                "foo.bar": "exact"
              }
            },
            "catchAll": {},
            "shortPartial": {},
            "longPartial": {},
            "exact": {}
          }
        })";

        lavi::Machine w_exact(WILDCARD_CPP_JSON);
        w_exact.start();
        w_exact.send("foo.bar");
        assert(w_exact.matches("exact"));

        lavi::Machine w_fallback(WILDCARD_CPP_JSON);
        w_fallback.registerGuard("allowLong", [](const lavi::Value&, const lavi::Value&) {
            return false;
        });
        w_fallback.start();
        w_fallback.send("foo.bar.baz");
        assert(w_fallback.matches("shortPartial"));

        lavi::Machine w_catchall(WILDCARD_CPP_JSON);
        w_catchall.start();
        w_catchall.send("UNRECOGNIZED");
        assert(w_catchall.matches("catchAll"));

        // Declarative xstate.assign multi-property context update test in C++
        const char* ASSIGN_CPP_JSON = R"({
          "initial": "counting",
          "context": { "count": 0, "foo": "bar" },
          "states": {
            "counting": {
              "on": {
                "WIN": {
                  "target": "won",
                  "actions": [
                    { "type": "xstate.assign", "assignment": { "count": 100, "foo": "win" } },
                    "verifyAssigned"
                  ]
                }
              }
            },
            "won": {}
          }
        })";

        bool cpp_assign_verified = false;
        lavi::Machine assign_cpp(ASSIGN_CPP_JSON);
        assign_cpp.registerAction("verifyAssigned", [&](const lavi::Value& context, const lavi::Value&) {
            double count_val = 0;
            std::string foo_val;
            assert(context.getProperty("count").getNumber(count_val) && count_val == 100.0);
            assert(context.getProperty("foo").getString(foo_val) && foo_val == "win");
            cpp_assign_verified = true;
        });
        assign_cpp.start();
        assign_cpp.send("WIN");
        assert(assign_cpp.matches("won"));
        assert(cpp_assign_verified);

        // State query tests: can(), hasTag(), isDone(), stop()
        const char* QUERIES_CPP_JSON = R"({
          "id": "queriesCpp",
          "initial": "idle",
          "states": {
            "idle": {
              "tags": ["ready", "standby"],
              "on": {
                "START": "working",
                "BLOCKED": {
                  "target": "working",
                  "guard": "deny"
                }
              }
            },
            "working": {
              "initial": "step1",
              "tags": "busy",
              "states": {
                "step1": {
                  "tags": ["phase1"],
                  "on": {
                    "FINISH": "#queriesCpp.done"
                  }
                }
              }
            },
            "done": {
              "type": "final",
              "tags": ["completed"]
            }
          }
        })";

        lavi::Machine q_cpp(QUERIES_CPP_JSON);
        q_cpp.registerGuard("deny", [](const lavi::Value&, const lavi::Value&) {
            return false;
        });
        q_cpp.start();

        assert(!q_cpp.isDone());
        assert(q_cpp.hasTag("ready"));
        assert(q_cpp.hasTag(std::string("standby")));
        assert(!q_cpp.hasTag("busy"));
        assert(q_cpp.can("START"));
        assert(!q_cpp.can("BLOCKED"));
        assert(!q_cpp.can(std::string("UNKNOWN")));

        q_cpp.send("START");
        assert(q_cpp.matches("working.step1"));
        assert(!q_cpp.isDone());
        assert(!q_cpp.hasTag("ready"));
        assert(q_cpp.hasTag("busy"));
        assert(q_cpp.hasTag("phase1"));
        assert(q_cpp.can("FINISH"));
        assert(!q_cpp.can("START"));

        q_cpp.send("FINISH");
        assert(q_cpp.matches("done"));
        assert(q_cpp.isDone());
        assert(q_cpp.hasTag("completed"));
        assert(!q_cpp.can("FINISH"));

        lavi::Machine q_stop(QUERIES_CPP_JSON);
        q_stop.start();
        assert(q_stop.can("START"));
        q_stop.stop();
        assert(!q_stop.can("START"));
        assert(!q_stop.isDone());
        q_stop.send("START");
        assert(q_stop.matches("idle"));

        // Snapshot serialization and restoration test
        const char* SNAPSHOT_CPP_JSON = R"({
          "id": "snapshotCppMachine",
          "initial": "idle",
          "context": { "count": 10, "label": "test" },
          "states": {
            "idle": {
              "on": {
                "INC": {
                  "actions": [
                    { "type": "xstate.assign", "assignment": { "count": 25, "label": "updated" } }
                  ],
                  "target": "active.sub1"
                }
              }
            },
            "active": {
              "initial": "sub1",
              "states": {
                "hist": { "type": "history" },
                "sub1": {
                  "on": { "GO2": "sub2" }
                },
                "sub2": {
                  "on": { "LEAVE": "#snapshotCppMachine.suspended" }
                }
              }
            },
            "suspended": {
              "on": { "BACK": "active.hist" }
            }
          }
        })";

        lavi::Machine snap_m1(SNAPSHOT_CPP_JSON);
        snap_m1.start();
        snap_m1.send("INC");
        assert(snap_m1.matches("active.sub1"));
        snap_m1.send("GO2");
        assert(snap_m1.matches("active.sub2"));
        snap_m1.send("LEAVE");
        assert(snap_m1.matches("suspended"));

        // Serialize snapshot
        std::string snap_str = snap_m1.getSnapshot();
        assert(!snap_str.empty());
        assert(snap_str.find("\"suspended\"") != std::string::npos);
        assert(snap_str.find("\"updated\"") != std::string::npos);

        // Restore into fresh machine before start()
        lavi::Machine snap_m2(SNAPSHOT_CPP_JSON);
        assert(snap_m2.restoreSnapshot(snap_str));
        assert(snap_m2.matches("suspended"));

        // Resuming from history must restore sub2
        snap_m2.send("BACK");
        assert(snap_m2.matches("active.sub2"));
        assert(!snap_m2.matches("active.sub1"));

        // Error handling validation: failed restoreSnapshot on unstarted machine must leave it startable
        lavi::Machine snap_err(SNAPSHOT_CPP_JSON);
        assert(!snap_err.restoreSnapshot(""));
        assert(!snap_err.restoreSnapshot("{ malformed json ]"));
        snap_err.start();
        assert(snap_err.matches("idle"));
        snap_err.send("INC");
        assert(snap_err.matches("active.sub1"));

        // Snapshot status parity test across active, done, and stopped states
        const char* STATUS_CPP_JSON = R"({
          "id": "statusCppFsm",
          "initial": "s1",
          "states": {
            "s1": { "on": { "STEP": "s2", "FINISH": "doneState" } },
            "s2": { "on": { "FINISH": "doneState" } },
            "doneState": { "type": "final" }
          }
        })";

        // 1. ACTIVE
        lavi::Machine cpp_act(STATUS_CPP_JSON);
        cpp_act.start();
        cpp_act.send("STEP");
        assert(cpp_act.matches("s2"));
        assert(!cpp_act.isDone());
        std::string act_snap = cpp_act.getSnapshot();
        assert(act_snap.find("\"status\":\"active\"") != std::string::npos);

        lavi::Machine cpp_act_res(STATUS_CPP_JSON);
        assert(cpp_act_res.restoreSnapshot(act_snap));
        assert(cpp_act_res.matches("s2"));
        assert(!cpp_act_res.isDone());
        cpp_act_res.send("FINISH");
        assert(cpp_act_res.matches("doneState"));
        assert(cpp_act_res.isDone());

        // 2. DONE
        lavi::Machine cpp_done(STATUS_CPP_JSON);
        cpp_done.start();
        cpp_done.send("FINISH");
        assert(cpp_done.matches("doneState"));
        assert(cpp_done.isDone());
        std::string done_snap = cpp_done.getSnapshot();
        assert(done_snap.find("\"status\":\"done\"") != std::string::npos);

        lavi::Machine cpp_done_res(STATUS_CPP_JSON);
        assert(cpp_done_res.restoreSnapshot(done_snap));
        assert(cpp_done_res.matches("doneState"));
        assert(cpp_done_res.isDone());
        std::string done_res_snap = cpp_done_res.getSnapshot();
        assert(done_res_snap.find("\"status\":\"done\"") != std::string::npos);
        // Ignores events
        cpp_done_res.send("STEP");
        assert(cpp_done_res.matches("doneState"));
        assert(cpp_done_res.isDone());

        // 3. STOPPED
        lavi::Machine cpp_stop(STATUS_CPP_JSON);
        cpp_stop.start();
        cpp_stop.send("STEP");
        cpp_stop.stop();
        assert(cpp_stop.matches("s2"));
        assert(!cpp_stop.isDone());
        std::string stop_snap = cpp_stop.getSnapshot();
        assert(stop_snap.find("\"status\":\"stopped\"") != std::string::npos);

        lavi::Machine cpp_stop_res(STATUS_CPP_JSON);
        assert(cpp_stop_res.restoreSnapshot(stop_snap));
        assert(cpp_stop_res.matches("s2"));
        assert(!cpp_stop_res.isDone());
        // Drops events
        cpp_stop_res.send("FINISH");
        assert(cpp_stop_res.matches("s2"));
        assert(!cpp_stop_res.matches("doneState"));
        std::string stop_res_snap = cpp_stop_res.getSnapshot();
        assert(stop_res_snap.find("\"status\":\"stopped\"") != std::string::npos);

        // 4. registerClock overload resolution test:
        //    - Captureless lambda returning uint64_t (previously ambiguous!)
        //    - Named const captureless lambda lvalue returning uint64_t
        //    - Captureless lambda returning unsigned long (e.g., Arduino millis())
        //    - Raw C function pointer
        //    - Capturing lambda + std::move
        const char* CLOCK_OVERLOAD_JSON = R"({
          "initial": "waiting",
          "states": {
            "waiting": {
              "after": { "100": "fired" }
            },
            "fired": {}
          }
        })";

        // 4a. Captureless lambda with explicit uint64_t return type
        g_fake_millis_ul = 1000;
        lavi::Machine clk_captureless_u64(CLOCK_OVERLOAD_JSON);
        clk_captureless_u64.registerClock([]() -> uint64_t { return fake_arduino_millis(); });
        clk_captureless_u64.start();
        assert(clk_captureless_u64.matches("waiting"));
        g_fake_millis_ul = 1099;
        clk_captureless_u64.tick();
        assert(clk_captureless_u64.matches("waiting"));
        g_fake_millis_ul = 1100;
        clk_captureless_u64.tick();
        assert(clk_captureless_u64.matches("fired"));

        // 4b. Named const captureless lambda lvalue returning uint64_t
        g_fake_millis_ul = 2000;
        const auto named_captureless_u64 = []() -> uint64_t { return fake_arduino_millis(); };
        lavi::Machine clk_named_u64(CLOCK_OVERLOAD_JSON);
        clk_named_u64.registerClock(named_captureless_u64);
        clk_named_u64.start();
        g_fake_millis_ul = 2100;
        clk_named_u64.tick();
        assert(clk_named_u64.matches("fired"));

        // 4c. Captureless lambda returning unsigned long (implicit conversion via ClockCallback)
        g_fake_millis_ul = 3000;
        lavi::Machine clk_captureless_ul(CLOCK_OVERLOAD_JSON);
        clk_captureless_ul.registerClock([]() { return fake_arduino_millis(); });
        clk_captureless_ul.start();
        g_fake_millis_ul = 3100;
        clk_captureless_ul.tick();
        assert(clk_captureless_ul.matches("fired"));

        // 4d. Raw C function pointer
        g_fake_millis_ul = 4000;
        lavi::Machine clk_raw_fn(CLOCK_OVERLOAD_JSON);
        clk_raw_fn.registerClock(raw_c_clock_fn);
        clk_raw_fn.start();
        g_fake_millis_ul = 4100;
        clk_raw_fn.tick();
        assert(clk_raw_fn.matches("fired"));

        // 4e. Capturing lambda + std::move of Machine
        uint64_t local_now = 5000;
        lavi::Machine clk_capturing(CLOCK_OVERLOAD_JSON);
        clk_capturing.registerClock([&local_now]() -> uint64_t { return local_now; });
        clk_capturing.start();
        lavi::Machine clk_capturing_moved = std::move(clk_capturing);
        local_now = 5100;
        clk_capturing_moved.tick();
        assert(clk_capturing_moved.matches("fired"));

    } catch (const std::exception& e) {
        std::cerr << "Test Failed with exception: " << e.what() << std::endl;
        return 1;
    }

    std::cout << "C++ Test Passed!" << std::endl;
    return 0;
}
