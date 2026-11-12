import 'package:lavi/lavi.dart';
import 'package:test/test.dart';

const String stoplightJson = '''
{
  "id": "stoplight",
  "initial": "red",
  "context": {
    "cycles": 1,
    "label": "main-street",
    "enabled": true
  },
  "states": {
    "red": {
      "tags": ["stop", "light"],
      "meta": { "color": "red", "duration": 1000 },
      "entry": [
        "onRed",
        { "type": "onParamAction", "params": { "step": 2, "tag": "go-green" } }
      ],
      "after": {
        "1000": "green"
      }
    },
    "green": {
      "tags": ["go"],
      "entry": ["onGreen"],
      "on": {
        "PEDESTRIAN_WAIT": {
          "target": "yellow",
          "guard": "canCross"
        },
        "PARAM_GUARD_EVENT": {
          "target": "yellow",
          "guard": { "type": "minCycles", "params": { "min": 1 } }
        },
        "FINISH": "doneState"
      },
      "after": {
        "customDelay": "yellow"
      }
    },
    "yellow": {
      "entry": ["onYellow"],
      "after": {
        "1000": "red"
      }
    },
    "doneState": {
      "type": "final",
      "output": { "result": "completed", "code": 200 }
    }
  }
}
''';

void main() {
  group('Lavi Dart FFI bindings', () {
    test('returns safe defaults when unstarted or closed', () {
      final machine = Machine(stoplightJson);
      expect(machine.isValid, isFalse);
      expect(machine.matches('red'), isFalse);
      expect(machine.can('PEDESTRIAN_WAIT'), isFalse);
      expect(machine.hasTag('stop'), isFalse);
      expect(machine.isDone, isFalse);
      expect(machine.state, equals(''));
      expect(machine.getSnapshot(), equals(''));
      expect(machine.getOutput(), equals(''));
      expect(machine.getMeta(), equals(''));
      expect(machine.getStateMeta('red'), equals(''));
      expect(machine.nextDeadline, isNull);

      machine.send('PEDESTRIAN_WAIT');
      machine.tick();
      machine.stop();
      machine.close();
      expect(machine.isValid, isFalse);
    });

    test('executes transitions, actions, guards, delays, and Value inspection', () {
      final clock = SimulatedClock(initialMs: 0);
      final machine = Machine(stoplightJson, clock: clock);

      var transitions = 0;
      String? seenLabel;
      bool? seenEnabled;
      double? seenParamStep;
      String? seenParamTag;
      double? lastEventPayload;

      machine.registerAction('onRed', (ctx, evt) {
        transitions += 1;
        seenLabel = ctx['label'].stringValue;
        seenEnabled = ctx['enabled'].boolValue;
        expect(ctx.isObject, isTrue);
        expect(ctx.isNull, isFalse);
        expect(ctx['nonexistent'].isNull, isTrue);
        expect(ctx['nonexistent']['deep'].isNull, isTrue);
        expect(ctx['label']['deep'].isNull, isTrue);
      });
      machine.registerAction('onGreen', (ctx, evt) {
        transitions += 1;
      });
      machine.registerAction('onYellow', (ctx, evt) {
        transitions += 1;
        if (evt.isObject) {
          lastEventPayload = evt['urgency'].numberValue;
        }
      });
      machine.registerActionWithParams('onParamAction', (ctx, evt, params) {
        seenParamStep = params['step'].numberValue;
        seenParamTag = params['tag'].stringValue;
      });
      machine.registerGuard('canCross', (ctx, evt) {
        return (ctx['cycles'].numberValue ?? 0) >= 1;
      });
      machine.registerGuardWithParams('minCycles', (ctx, evt, params) {
        final cycles = ctx['cycles'].numberValue ?? 0;
        final minVal = params['min'].numberValue ?? 999;
        return cycles >= minVal;
      });
      machine.registerDelay('customDelay', (ctx, evt) => 2500);

      machine.start();
      expect(machine.isValid, isTrue);
      expect(machine.matches('red'), isTrue);
      expect(machine.hasTag('stop'), isTrue);
      expect(machine.hasTag('go'), isFalse);
      expect(transitions, equals(1));
      expect(seenLabel, equals('main-street'));
      expect(seenEnabled, isTrue);
      expect(seenParamStep, equals(2.0));
      expect(seenParamTag, equals('go-green'));
      expect(machine.nextDeadline, equals(1000));
      expect(machine.getMeta(), contains('red'));
      expect(machine.getStateMeta('red'), contains('1000'));

      clock.increment(1000);
      expect(clock.now, equals(1000));
      expect(machine.matches('green'), isTrue);
      expect(machine.hasTag('go'), isTrue);
      expect(transitions, equals(2));
      expect(machine.can('PEDESTRIAN_WAIT'), isTrue);
      expect(machine.can('PARAM_GUARD_EVENT'), isTrue);
      expect(machine.nextDeadline, equals(3500));

      machine.sendJson('{"type":"PEDESTRIAN_WAIT","urgency":42}');
      expect(machine.matches('yellow'), isTrue);
      expect(transitions, equals(3));
      expect(lastEventPayload, equals(42.0));

      clock.set(2000);
      expect(machine.matches('red'), isTrue);
      expect(transitions, equals(4));

      // Closing machine detaches it from SimulatedClock so subsequent clock increments are safe
      machine.close();
      clock.increment(5000);
      expect(transitions, equals(4));
      clock.close();
    });

    test('restores snapshot on unstarted machine and serializes final output', () {
      final clock1 = SimulatedClock(initialMs: 0);
      final machine1 = Machine(stoplightJson, clock: clock1);
      machine1.registerDelay('customDelay', (ctx, evt) => 5000);
      machine1.start();
      clock1.increment(1000);
      expect(machine1.matches('green'), isTrue);
      final snap = machine1.getSnapshot();
      expect(snap, isNotEmpty);

      final clock2 = SimulatedClock(initialMs: 1000);
      final machine2 = Machine(stoplightJson, clock: clock2);
      var yellowEntered = false;
      machine2.registerDelay('customDelay', (ctx, evt) => 5000);
      machine2.registerAction('onYellow', (ctx, evt) {
        yellowEntered = true;
      });
      machine2.registerGuardWithParams('minCycles', (ctx, evt, params) {
        return (ctx['cycles'].numberValue ?? 0) >= (params['min'].numberValue ?? 0);
      });

      expect(machine2.restoreSnapshot(snap), isTrue);
      expect(machine2.matches('green'), isTrue);

      machine2.send('PARAM_GUARD_EVENT');
      expect(machine2.matches('yellow'), isTrue);
      expect(yellowEntered, isTrue);

      clock2.increment(1000);
      expect(machine2.matches('red'), isTrue);
      clock2.increment(1000);
      expect(machine2.matches('green'), isTrue);
      machine2.send('FINISH');
      expect(machine2.isDone, isTrue);
      expect(machine2.getOutput(), contains('completed'));

      machine1.close();
      clock1.close();
      machine2.close();
      clock2.close();
    });

    test('supports callback replacement and custom clock', () {
      var nowMs = 100;
      final machine = Machine(stoplightJson);
      var actionHits = 0;
      machine.registerDelay('customDelay', (ctx, evt) => 5000);
      machine.registerAction('onRed', (ctx, evt) {
        actionHits += 10;
      });
      machine.registerAction('onRed', (ctx, evt) {
        actionHits += 1;
      });
      machine.registerClock(() => nowMs);

      machine.start();
      expect(actionHits, equals(1));
      expect(machine.nextDeadline, equals(1100));

      nowMs = 1100;
      machine.tick();
      expect(machine.matches('green'), isTrue);

      machine.stop();
      machine.send('FINISH');
      expect(machine.isDone, isFalse);

      machine.close();
    });

    test('supports registerInspector callback', () {
      final clock = SimulatedClock(initialMs: 0);
      final machine = Machine(stoplightJson, clock: clock);
      machine.registerDelay('customDelay', (ctx, evt) => 5000);
      final inspected = <String>[];
      machine.registerInspector((json) {
        inspected.add(json);
      });
      machine.start();
      clock.increment(1000);
      machine.send('FINISH');
      expect(machine.isDone, isTrue);
      expect(inspected.any((e) => e.contains('@xstate.microstep')), isTrue);
      expect(inspected.any((e) => e.contains('@xstate.snapshot')), isTrue);
      machine.close();
      clock.close();
    });

    test('supports pure transitions, output resolver, final leaf output, and invoke callbacks', () {
      const td21Json = '''
      {
        "id": "td21Dart",
        "initial": "active",
        "context": { "total": 18 },
        "states": {
          "active": {
            "initial": "working",
            "invoke": { "id": "dartWorker", "src": "dartTask", "input": { "rate": 9 } },
            "states": {
              "working": {
                "entry": ["logWorking"],
                "on": { "DONE_LEAF": "leafFinal" }
              },
              "leafFinal": {
                "type": "final",
                "output": { "subStatus": "dartLeafDone" }
              }
            },
            "on": { "FINISH_ALL": "completed" }
          },
          "completed": { "type": "final" }
        }
      }
      ''';

      final machine = Machine(td21Json);
      final startedInvokes = <String>[];
      final stoppedInvokes = <String>[];

      machine.registerOutputResolver('', (ctx, evt) {
        final total = (ctx['total'].numberValue ?? 0).toInt();
        return '{"doubled":${total * 2}}';
      });
      machine.registerInvokeCallbacks(
        onStart: (id, src, inp, ctx, evt) {
          final rate = (inp['rate'].numberValue ?? 0).toInt();
          startedInvokes.add('$id:$src:$rate');
        },
        onStop: (id) {
          stoppedInvokes.add(id);
        },
      );

      final initPure = machine.initialTransitionJson();
      expect(initPure, contains('"value":{"active":"working"}'));
      expect(startedInvokes, isEmpty);
      expect(machine.isValid, isFalse);

      expect(machine.start(), isTrue);
      expect(machine.isValid, isTrue);
      expect(startedInvokes, equals(['dartWorker:dartTask:9']));

      final liveSnap = machine.getSnapshot();
      final nextPure = machine.transitionPureJson(liveSnap, '{"type":"FINISH_ALL"}');
      expect(nextPure, contains('"value":"completed"'));
      expect(machine.matches('active.working'), isTrue);
      expect(stoppedInvokes, isEmpty);

      expect(machine.getFinalLeafOutput(), equals(''));
      machine.send('DONE_LEAF');
      expect(machine.matches('active.leafFinal'), isTrue);
      expect(machine.getFinalLeafOutput(), contains('dartLeafDone'));

      machine.send('FINISH_ALL');
      expect(machine.isDone, isTrue);
      expect(stoppedInvokes, equals(['dartWorker']));
      expect(machine.getOutput(), contains('"doubled":36'));

      machine.close();
    });
  });
}
