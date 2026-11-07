#include "lavi.hpp"
#include <cassert>
#include <iostream>

static const char* MACHINE_JSON = R"({
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
})";

int main() {
  int transitions = 0;

  lavi::SimulatedClock clock(0);
  lavi::Machine machine(MACHINE_JSON, clock);

  auto on_entry = [&transitions](const lavi::Value&, const lavi::Value&) {
    transitions += 1;
  };
  machine.registerAction("onRed", on_entry);
  machine.registerAction("onGreen", on_entry);
  machine.registerAction("onYellow", on_entry);

  machine.registerGuard("canCross", [](const lavi::Value& ctx, const lavi::Value&) {
    double cycles = 0.0;
    ctx.getProperty("cycles").getNumber(cycles);
    return cycles >= 1.0;
  });

  // 1. Start in "red"
  machine.start();
  assert(machine.matches("red"));
  assert(transitions == 1);

  // 2. Advance 1000ms -> "green" (via `after: 1000`)
  clock.increment(1000);
  assert(machine.matches("green"));
  assert(transitions == 2);

  // 3. Send guarded "PEDESTRIAN_WAIT" -> "yellow"
  machine.send("PEDESTRIAN_WAIT");
  assert(machine.matches("yellow"));
  assert(transitions == 3);

  // 4. Advance 1000ms -> "red" (via `after: 1000`)
  clock.increment(1000);
  assert(machine.matches("red"));
  assert(transitions == 4);

  std::cout << "C++ example passed (transitions: " << transitions << ")\n";
  return 0;
}
