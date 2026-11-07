from lavi import Machine, SimulatedClock, Value

MACHINE_JSON = """{
  "id": "stoplight",
  "initial": "red",
  "context": { "cycles": 1 },
  "states": {
    "red": {
      "entry": ["onRed"],
      "after": { "1000": "green" }
    },
    "green": {
      "entry": ["onGreen"],
      "on": {
        "PEDESTRIAN_WAIT": {
          "target": "yellow",
          "guard": "canCross"
        }
      },
      "after": { "1000": "yellow" }
    },
    "yellow": {
      "entry": ["onYellow"],
      "after": { "1000": "red" }
    }
  }
}"""


def main() -> None:
    transitions = 0
    clock = SimulatedClock(initial_ms=0)

    with Machine(MACHINE_JSON, clock=clock) as machine:
        @machine.action("onRed")
        @machine.action("onGreen")
        @machine.action("onYellow")
        def on_entry(_ctx: Value, _evt: Value) -> None:
            nonlocal transitions
            transitions += 1

        @machine.guard("canCross")
        def can_cross(ctx: Value, _evt: Value) -> bool:
            return (ctx["cycles"].as_int() or 0) >= 1

        # 1. Start in "red"
        machine.start()
        assert machine.matches("red")
        assert transitions == 1

        # 2. Advance 1000ms -> "green" (via `after: 1000`)
        clock.increment(1000)
        assert machine.matches("green")
        assert transitions == 2

        # 3. Send guarded "PEDESTRIAN_WAIT" -> "yellow"
        machine.send("PEDESTRIAN_WAIT")
        assert machine.matches("yellow")
        assert transitions == 3

        # 4. Advance 1000ms -> "red" (via `after: 1000`)
        clock.increment(1000)
        assert machine.matches("red")
        assert transitions == 4

    print(f"Python example passed (transitions: {transitions})")


if __name__ == "__main__":
    main()
