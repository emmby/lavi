# Lavi for Python (`lavi`)

Fast, portable **XState v5** statechart engine for Python, powered by Lavi's Rust core via PyO3 (`abi3`).

## Installation

```bash
pip install lavi
```

## Quick Start

```python
import json
from lavi import Machine, SimulatedClock, Value

MACHINE_JSON = json.dumps({
    "id": "stoplight",
    "initial": "red",
    "context": {"cycles": 1},
    "states": {
        "red": {"after": {"1000": "green"}},
        "green": {
            "on": {"PEDESTRIAN_WAIT": {"target": "yellow", "guard": "canCross"}},
            "after": {"1000": "yellow"},
        },
        "yellow": {"after": {"1000": "red"}},
    },
})

clock = SimulatedClock(initial_ms=0)
with Machine(MACHINE_JSON, clock=clock) as machine:
    @machine.guard("canCross")
    def can_cross(ctx: Value, _evt: Value) -> bool:
        return (ctx["cycles"].as_int() or 0) >= 1

    machine.start()
    assert machine.matches("red")

    clock.increment(1000)
    assert machine.matches("green")

    machine.send("PEDESTRIAN_WAIT")
    assert machine.matches("yellow")
```

## License

MIT
