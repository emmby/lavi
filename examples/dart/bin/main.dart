import 'package:lavi/lavi.dart';

const String machineJson = '''
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
''';

void _check(bool condition, String message) {
  if (!condition) {
    throw StateError('Assertion failed: $message');
  }
}

void main() {
  var transitions = 0;
  final clock = SimulatedClock(initialMs: 0);
  final machine = Machine(machineJson, clock: clock);

  void onEntry(Value ctx, Value evt) {
    transitions += 1;
  }

  machine.registerAction('onRed', onEntry);
  machine.registerAction('onGreen', onEntry);
  machine.registerAction('onYellow', onEntry);

  machine.registerGuard('canCross', (ctx, evt) {
    return (ctx['cycles'].numberValue ?? 0) >= 1.0;
  });

  // 1. Start in "red"
  machine.start();
  _check(machine.matches('red'), 'expected red');
  _check(transitions == 1, 'expected 1 transition');

  // 2. Advance 1000ms -> "green" (via `after: 1000`)
  clock.increment(1000);
  _check(machine.matches('green'), 'expected green');
  _check(transitions == 2, 'expected 2 transitions');

  // 3. Send guarded "PEDESTRIAN_WAIT" -> "yellow"
  machine.send('PEDESTRIAN_WAIT');
  _check(machine.matches('yellow'), 'expected yellow');
  _check(transitions == 3, 'expected 3 transitions');

  // 4. Advance 1000ms -> "red" (via `after: 1000`)
  clock.increment(1000);
  _check(machine.matches('red'), 'expected red');
  _check(transitions == 4, 'expected 4 transitions');

  machine.close();
  clock.close();

  print('Dart example passed (transitions: $transitions)');
}
