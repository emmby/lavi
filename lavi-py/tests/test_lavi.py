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
