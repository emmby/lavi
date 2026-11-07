import gc
import json
import weakref
import pytest
from lavi import Machine, SimulatedClock, Value

STOPLIGHT_JSON = """{
  "id": "trafficLight",
  "initial": "green",
  "context": {
    "cycles": 0,
    "ratio": 1.5,
    "label": "main",
    "active": true,
    "empty": null,
    "items": ["a", "b"],
    "nested": { "ok": true }
  },
  "states": {
    "green": {
      "tags": ["go"],
      "after": { "1000": "yellow" },
      "on": { "TIMER": "yellow" }
    },
    "yellow": { "on": { "TIMER": "red" } },
    "red": { "type": "final" }
  }
}"""

def test_invalid_json_raises_value_error():
    with pytest.raises(ValueError):
        Machine("{bad json")

def test_unstarted_machine_drops_events_and_matches_nothing():
    with Machine(STOPLIGHT_JSON) as m:
        assert not m.matches("green")
        assert not m.can("TIMER")
        assert not m.has_tag("go")
        assert not m.is_done()
        m.send("TIMER")
        m.start()
        assert m.matches("green")

def test_lifecycle_transitions_tags_done_and_snapshots():
    clock = SimulatedClock(100)
    with Machine(STOPLIGHT_JSON, clock=clock) as m:
        m.start()
        assert m.matches("green")
        assert m.can("TIMER")
        assert not m.can("UNKNOWN")
        assert m.has_tag("go")
        assert m.get_next_deadline() == 1100

        snap = m.get_snapshot()
        clock.increment(1000)
        assert m.matches("yellow")
        assert not m.has_tag("go")

        m.send("TIMER")
        assert m.matches("red")
        assert m.is_done()

        m.restore_snapshot(snap)
        assert m.matches("green")
        with pytest.raises(ValueError):
            m.restore_snapshot("{invalid_snapshot")

def test_stop_and_closed_machine_behavior():
    m = Machine(STOPLIGHT_JSON)
    m.start()
    m.stop()
    m.send("TIMER")
    assert m.matches("green")
    m.close()
    with pytest.raises(RuntimeError):
        m.send("TIMER")

CALLBACK_MACHINE_JSON = """{
  "id": "cbMachine",
  "initial": "idle",
  "context": {
    "count": 2,
    "ratio": 1.5,
    "name": "sensor",
    "ready": true,
    "nothing": null,
    "items": [10, 20],
    "meta": { "tag": "v1" }
  },
  "states": {
    "idle": {
      "on": {
        "CHECK": [
          { "target": "active", "guard": "canActivate", "actions": ["onActivate", "secondAction"] },
          { "target": "fallback" }
        ]
      }
    },
    "active": {
      "after": {
        "100": { "target": "step2", "actions": ["onTick100"] }
      },
      "on": { " Chain": "done" }
    },
    "step2": {
      "after": {
        "100": { "target": "done", "actions": ["onTick200"] }
      }
    },
    "fallback": {},
    "done": { "type": "final" }
  }
}"""

def test_callbacks_value_ergonomics_and_retained_value_safety():
    retained = {}
    with Machine(CALLBACK_MACHINE_JSON) as m:
        @m.guard("canActivate")
        def check(ctx, evt):
            assert ctx["count"] == 2
            assert ctx["count"].as_int() == 2
            assert ctx["count"].is_number()
            assert ctx["ratio"].as_float() == 1.5
            assert ctx["ratio"].is_number()
            assert ctx["name"].as_str() == "sensor"
            assert ctx["name"].is_str()
            assert bool(ctx["ready"]) is True
            assert ctx["ready"].is_bool()
            assert bool(ctx["nothing"]) is False
            assert ctx["nothing"].is_null()
            assert ctx["missing"]["deep"].is_null()
            assert bool(ctx["missing"]) is False
            assert ctx["items"].is_array()
            assert ctx["items"][1].as_int() == 20
            assert ctx["items"][99].is_null()
            assert len(ctx["items"]) == 2
            assert ctx["items"][-1].as_int() == 20
            assert ctx["items"][-2].as_int() == 10
            assert ctx["items"][-99].is_null()
            assert bool(ctx["items"][-99]) is False
            assert "meta" in ctx
            assert ctx["meta"].is_object()
            assert ctx["meta"]["tag"] == "v1"
            assert ctx.get("meta")["tag"] == "v1"
            assert ctx.get("nonexistent", "fallback") == "fallback"
            assert evt["type"] == "CHECK"
            assert evt["attempt"].as_int() == 3
            retained["ctx"] = ctx
            retained["evt"] = evt
            return True

        m.register_action("onActivate", lambda ctx, evt: retained.setdefault("activated", ctx.to_python()))
        m.start()
        m.send("CHECK", payload={"attempt": 3})
        assert m.matches("active")

    # Retained Value objects remain safe after Machine is closed
    assert retained["ctx"]["name"].as_str() == "sensor"
    assert retained["evt"]["attempt"].as_int() == 3
    assert retained["activated"]["items"] == [10, 20]

def test_reentrant_send_matches_and_stop_inside_action_callback():
    with Machine(CALLBACK_MACHINE_JSON) as m:
        m.register_guard("canActivate", lambda ctx, evt: True)
        def reentrant_action(ctx, evt):
            assert m.matches("active")
            m.send({"type": " Chain"})
        m.register_action("onActivate", reentrant_action)
        m.start()
        m.send("CHECK")
        assert m.matches("done")
        assert m.is_done()

def test_callback_exception_short_circuits_step_and_simulated_clock():
    clock = SimulatedClock(0)
    second_called = False
    tick200_called = False
    with Machine(CALLBACK_MACHINE_JSON, clock=clock) as m:
        m.register_guard("canActivate", lambda ctx, evt: True)
        def fail_on_tick100(ctx, evt):
            raise RuntimeError("boom at 100ms")
        def on_tick200(ctx, evt):
            nonlocal tick200_called
            tick200_called = True
        m.register_action("onTick100", fail_on_tick100)
        m.register_action("onTick200", on_tick200)
        m.start()
        m.send("CHECK")
        with pytest.raises(RuntimeError, match="boom at 100ms"):
            clock.set(500)
        assert clock.now() == 100
        assert not tick200_called

def test_gc_collects_reference_cycle_between_machine_and_callback():
    def make_cycle():
        m = Machine(CALLBACK_MACHINE_JSON)
        m.register_action("onActivate", lambda ctx, evt: m.send(" Chain"))
        return weakref.ref(m)
    ref = make_cycle()
    gc.collect()
    assert ref() is None

def test_nested_callback_can_query_and_reentrant_send():
    nested_machine_json = json.dumps({
        "id": "nested",
        "initial": "idle",
        "states": {
            "idle": {
                "on": {
                    "START": {
                        "target": "active",
                        "actions": ["onStart"]
                    }
                }
            },
            "active": {
                "on": {
                    "CHECK": {
                        "guard": "checkGuard",
                        "target": "active"
                    },
                    "NEXT": {
                        "target": "finished"
                    }
                }
            },
            "finished": {
                "type": "final"
            }
        }
    })

    guard_called = False
    action_completed = False

    with Machine(nested_machine_json) as m:
        @m.guard("checkGuard")
        def check_guard(ctx, evt):
            nonlocal guard_called
            guard_called = True
            assert m.matches("active")
            return True

        @m.action("onStart")
        def on_start(ctx, evt):
            nonlocal action_completed
            can_check = m.can("CHECK")
            assert can_check is True
            # If in_callback was reset by check_guard, this will panic with BorrowError!
            assert m.matches("active")
            m.send("NEXT")
            action_completed = True

        m.start()
        assert m.matches("idle")
        m.send("START")
        assert guard_called
        assert action_completed
        assert m.matches("finished")

def test_simulated_clock_deduplication():
    timer_json = json.dumps({
        "id": "timer",
        "initial": "a",
        "states": {
            "a": {
                "after": {
                    "100": {
                        "target": "b",
                        "actions": ["onTick"]
                    }
                }
            },
            "b": {}
        }
    })
    ticks = 0
    clock = SimulatedClock(0)
    with Machine(timer_json, clock=clock) as m:
        m.register_clock(clock)

        @m.action("onTick")
        def on_tick(ctx, evt):
            nonlocal ticks
            ticks += 1

        m.start()
        clock.set(150)
        assert m.matches("b")
        assert ticks == 1

def test_clock_callable_exception_propagation_and_recovery():
    timer_json = json.dumps({
        "id": "timer_err",
        "initial": "idle",
        "states": {
            "idle": {
                "on": {
                    "GO": "waiting"
                }
            },
            "waiting": {
                "after": {
                    "100": "done"
                }
            },
            "done": {
                "type": "final"
            }
        }
    })
    should_fail = False
    def failing_clock():
        if should_fail:
            raise ValueError("clock broken")
        return 100

    with Machine(timer_json) as m:
        m.register_clock(failing_clock)
        m.start()
        assert m.matches("idle")

        should_fail = True
        with pytest.raises(ValueError, match="clock broken"):
            m.send("GO")

        # Machine aborted state was reset when error was raised,
        # so subsequent operations work once clock is healthy:
        should_fail = False
        # Can query matches and can() without exception
        assert not m.is_done()

def test_decorators_action_guard_and_clock_syntax():
    machine_json = json.dumps({
        "id": "dec",
        "initial": "a",
        "states": {
            "a": {
                "on": {
                    "STEP": {
                        "guard": "isStepAllowed",
                        "target": "b",
                        "actions": ["onStep"]
                    }
                }
            },
            "b": {
                "after": {
                    "50": "c"
                }
            },
            "c": {
                "type": "final"
            }
        }
    })
    step_action_called = False
    with Machine(machine_json) as m:
        @m.guard("isStepAllowed")
        def check(ctx, evt):
            return True

        @m.action("onStep")
        def act(ctx, evt):
            nonlocal step_action_called
            step_action_called = True

        @m.clock
        def my_clock():
            return 100

        m.start()
        assert m.can("STEP")
        m.send("STEP")
        assert step_action_called
        assert m.matches("b")

def test_clock_decorator_with_parens():
    with Machine(STOPLIGHT_JSON) as m:
        @m.clock()
        def my_clock():
            return 42
        m.start()
        assert m.matches("green")



