import XCTest
@testable import Lavi

final class LaviTests: XCTestCase {
    private let stoplightJson = """
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
    """

    func testUnstartedAndClosedSafeDefaults() {
        let machine = Machine(jsonConfig: stoplightJson)
        XCTAssertFalse(machine.isValid)
        XCTAssertFalse(machine.matches("red"))
        XCTAssertFalse(machine.can("PEDESTRIAN_WAIT"))
        XCTAssertFalse(machine.hasTag("stop"))
        XCTAssertFalse(machine.isDone)
        XCTAssertEqual(machine.state, "")
        XCTAssertEqual(machine.getSnapshot(), "")
        XCTAssertEqual(machine.getOutput(), "")
        XCTAssertEqual(machine.getMeta(), "")
        XCTAssertEqual(machine.getStateMeta("red"), "")
        XCTAssertNil(machine.nextDeadline)

        machine.send("PEDESTRIAN_WAIT")
        machine.tick()
        machine.stop()
        machine.close()
        XCTAssertFalse(machine.isValid)
    }

    func testTransitionsActionsGuardsAndValueInspection() {
        let clock = SimulatedClock(initialMs: 0)
        let machine = Machine(jsonConfig: stoplightJson, clock: clock)

        var transitions = 0
        var seenLabel: String?
        var seenEnabled: Bool?
        var seenParamStep: Double?
        var seenParamTag: String?
        var lastEventPayload: Double?

        machine.registerAction("onRed") { ctx, _ in
            transitions += 1
            seenLabel = ctx["label"].stringValue
            seenEnabled = ctx["enabled"].boolValue
            XCTAssertTrue(ctx.isObject)
            XCTAssertFalse(ctx.isNull)
            XCTAssertTrue(ctx["nonexistent"].isNull)
            XCTAssertTrue(ctx["nonexistent"]["deep"].isNull)
            XCTAssertTrue(ctx["label"]["deep"].isNull)
        }
        machine.registerAction("onGreen") { _, _ in
            transitions += 1
        }
        machine.registerAction("onYellow") { _, evt in
            transitions += 1
            if evt.isObject {
                lastEventPayload = evt["urgency"].numberValue
            }
        }
        machine.registerActionWithParams("onParamAction") { _, _, params in
            seenParamStep = params["step"].numberValue
            seenParamTag = params["tag"].stringValue
        }
        machine.registerGuard("canCross") { ctx, _ in
            return (ctx["cycles"].numberValue ?? 0) >= 1
        }
        machine.registerGuardWithParams("minCycles") { ctx, _, params in
            let cycles = ctx["cycles"].numberValue ?? 0
            let minVal = params["min"].numberValue ?? 999
            return cycles >= minVal
        }
        machine.registerDelay("customDelay") { _, _ in
            return 2500
        }

        machine.start()
        XCTAssertTrue(machine.isValid)
        XCTAssertTrue(machine.matches("red"))
        XCTAssertTrue(machine.hasTag("stop"))
        XCTAssertFalse(machine.hasTag("go"))
        XCTAssertEqual(transitions, 1)
        XCTAssertEqual(seenLabel, "main-street")
        XCTAssertEqual(seenEnabled, true)
        XCTAssertEqual(seenParamStep, 2.0)
        XCTAssertEqual(seenParamTag, "go-green")
        XCTAssertEqual(machine.nextDeadline, 1000)
        XCTAssertTrue(machine.getMeta().contains("red"))
        XCTAssertTrue(machine.getStateMeta("red").contains("1000"))

        clock.increment(1000)
        XCTAssertEqual(clock.now, 1000)
        XCTAssertTrue(machine.matches("green"))
        XCTAssertTrue(machine.hasTag("go"))
        XCTAssertEqual(transitions, 2)
        XCTAssertTrue(machine.can("PEDESTRIAN_WAIT"))
        XCTAssertTrue(machine.can("PARAM_GUARD_EVENT"))
        XCTAssertEqual(machine.nextDeadline, 3500)

        machine.sendJson(#"{"type":"PEDESTRIAN_WAIT","urgency":42}"#)
        XCTAssertTrue(machine.matches("yellow"))
        XCTAssertEqual(transitions, 3)
        XCTAssertEqual(lastEventPayload, 42.0)

        clock.set(2000)
        XCTAssertTrue(machine.matches("red"))
        XCTAssertEqual(transitions, 4)

        // Closing machine detaches it from SimulatedClock so subsequent clock increments are safe
        machine.close()
        clock.increment(5000)
        XCTAssertEqual(transitions, 4)
    }

    func testSnapshotRestoreOnUnstartedMachineAndFinalOutput() {
        let clock1 = SimulatedClock(initialMs: 0)
        let machine1 = Machine(jsonConfig: stoplightJson, clock: clock1)
        machine1.registerDelay("customDelay") { _, _ in 5000 }
        machine1.start()
        clock1.increment(1000)
        XCTAssertTrue(machine1.matches("green"))
        let snap = machine1.getSnapshot()
        XCTAssertFalse(snap.isEmpty)

        // Restore into an unstarted machine
        let clock2 = SimulatedClock(initialMs: 1000)
        let machine2 = Machine(jsonConfig: stoplightJson, clock: clock2)
        var yellowEntered = false
        machine2.registerDelay("customDelay") { _, _ in 5000 }
        machine2.registerAction("onYellow") { _, _ in
            yellowEntered = true
        }
        machine2.registerGuardWithParams("minCycles") { ctx, _, params in
            return (ctx["cycles"].numberValue ?? 0) >= (params["min"].numberValue ?? 0)
        }
        XCTAssertTrue(machine2.restoreSnapshot(snap))
        XCTAssertTrue(machine2.matches("green"))

        machine2.send("PARAM_GUARD_EVENT")
        XCTAssertTrue(machine2.matches("yellow"))
        XCTAssertTrue(yellowEntered)

        // Advance to red, then green, then finish
        clock2.increment(1000)
        XCTAssertTrue(machine2.matches("red"))
        clock2.increment(1000)
        XCTAssertTrue(machine2.matches("green"))
        machine2.send("FINISH")
        XCTAssertTrue(machine2.isDone)
        XCTAssertTrue(machine2.getOutput().contains("completed"))
    }

    func testCallbackReplacementAndCustomClock() {
        var nowMs: UInt64 = 100
        let machine = Machine(jsonConfig: stoplightJson)
        var actionHits = 0
        machine.registerDelay("customDelay") { _, _ in 5000 }
        machine.registerAction("onRed") { _, _ in
            actionHits += 10
        }
        // Replace onRed before start
        machine.registerAction("onRed") { _, _ in
            actionHits += 1
        }
        machine.registerClock {
            return nowMs
        }
        machine.start()
        XCTAssertEqual(actionHits, 1)
        XCTAssertEqual(machine.nextDeadline, 1100)

        nowMs = 1100
        machine.tick()
        XCTAssertTrue(machine.matches("green"))

        machine.stop()
        machine.send("FINISH")
        XCTAssertFalse(machine.isDone)
    }

    func testInspectorCallback() {
        let clock = SimulatedClock(initialMs: 0)
        let machine = Machine(jsonConfig: stoplightJson, clock: clock)
        machine.registerDelay("customDelay") { _, _ in 5000 }
        var inspected: [String] = []
        machine.registerInspector { json in
            inspected.append(json)
        }
        machine.start()
        clock.increment(1000)
        machine.send("FINISH")
        XCTAssertTrue(machine.isDone)
        XCTAssertTrue(inspected.contains(where: { $0.contains("@xstate.microstep") }))
        XCTAssertTrue(inspected.contains(where: { $0.contains("@xstate.snapshot") }))
    }
}
