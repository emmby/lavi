using System;
using Lavi;

const string MachineJson = """
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
""";

static void Check(bool condition, string message)
{
    if (!condition)
    {
        throw new InvalidOperationException($"Assertion failed: {message}");
    }
}

var transitions = 0;
using var clock = new SimulatedClock(0);
using var machine = new Machine(MachineJson, clock: clock);

void OnEntry(Value ctx, Value evt)
{
    transitions += 1;
}

machine
    .RegisterAction("onRed", OnEntry)
    .RegisterAction("onGreen", OnEntry)
    .RegisterAction("onYellow", OnEntry)
    .RegisterGuard("canCross", (ctx, _) => (ctx["cycles"].AsInt32 ?? 0) >= 1);

// 1. Start in "red"
machine.Start();
Check(machine.Matches("red"), "expected red");
Check(transitions == 1, "expected 1 transition");

// 2. Advance 1000ms -> "green" (via `after: 1000`)
clock.Increment(TimeSpan.FromMilliseconds(1000));
Check(machine.Matches("green"), "expected green");
Check(transitions == 2, "expected 2 transitions");

// 3. Send guarded "PEDESTRIAN_WAIT" -> "yellow"
machine.Send("PEDESTRIAN_WAIT");
Check(machine.Matches("yellow"), "expected yellow");
Check(transitions == 3, "expected 3 transitions");

// 4. Advance 1000ms -> "red" (via `after: 1000`)
clock.Increment(1000);
Check(machine.Matches("red"), "expected red");
Check(transitions == 4, "expected 4 transitions");

Console.WriteLine($"C# example passed (transitions: {transitions})");
