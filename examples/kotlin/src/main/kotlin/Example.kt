import io.lavi.*

private const val MACHINE_JSON = """{
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

fun main() {
    var transitions = 0

    SimulatedClock(0).use { clock ->
        Machine(MACHINE_JSON, clock).use { machine ->
            val onEntry: (Value, Value) -> Unit = { _, _ -> transitions += 1 }
            machine.onAction("onRed", onEntry)
            machine.onAction("onGreen", onEntry)
            machine.onAction("onYellow", onEntry)

            machine.onGuard("canCross") { ctx, _ ->
                (ctx["cycles"].asLongOrNull() ?: 0L) >= 1L
            }

            // 1. Start in "red"
            machine.start()
            check(machine.matches("red")) { "expected state 'red'" }
            check(transitions == 1) { "expected 1 transition" }

            // 2. Advance 1000ms -> "green" (via `after: 1000`)
            clock.increment(1000)
            check(machine.matches("green")) { "expected state 'green'" }
            check(transitions == 2) { "expected 2 transitions" }

            // 3. Send guarded "PEDESTRIAN_WAIT" -> "yellow"
            machine.send("PEDESTRIAN_WAIT")
            check(machine.matches("yellow")) { "expected state 'yellow'" }
            check(transitions == 3) { "expected 3 transitions" }

            // 4. Advance 1000ms -> "red" (via `after: 1000`)
            clock.increment(1000)
            check(machine.matches("red")) { "expected state 'red'" }
            check(transitions == 4) { "expected 4 transitions" }
        }
    }

    println("Kotlin example passed (transitions: $transitions)")
}
