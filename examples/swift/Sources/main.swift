import Lavi

let machineJson = """
{
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
}
"""

var transitions = 0
let clock = SimulatedClock(initialMs: 0)
let machine = Machine(jsonConfig: machineJson, clock: clock)

let onEntry: ActionCallback = { _, _ in
    transitions += 1
}

machine.registerAction("onRed", callback: onEntry)
machine.registerAction("onGreen", callback: onEntry)
machine.registerAction("onYellow", callback: onEntry)

machine.registerGuard("canCross") { ctx, _ in
    return (ctx["cycles"].numberValue ?? 0) >= 1.0
}

// 1. Start in "red"
machine.start()
precondition(machine.matches("red"))
precondition(transitions == 1)

// 2. Advance 1000ms -> "green" (via `after: 1000`)
clock.increment(1000)
precondition(machine.matches("green"))
precondition(transitions == 2)

// 3. Send guarded "PEDESTRIAN_WAIT" -> "yellow"
machine.send("PEDESTRIAN_WAIT")
precondition(machine.matches("yellow"))
precondition(transitions == 3)

// 4. Advance 1000ms -> "red" (via `after: 1000`)
clock.increment(1000)
precondition(machine.matches("red"))
precondition(transitions == 4)

machine.close()
clock.close()
print("Swift example passed (transitions: \(transitions))")
