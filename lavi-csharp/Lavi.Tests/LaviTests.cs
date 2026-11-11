using System;
using System.Collections.Generic;
using Xunit;

namespace Lavi.Tests;

public class LaviTests
{
    private const string StoplightJson = """
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
    """;

    [Fact]
    public void UnstartedOrDisposed_ReturnsSafeDefaultsAndHandlesNullInputs()
    {
        var machine = new Machine(StoplightJson);
        Assert.False(machine.IsValid);
        Assert.False(machine.Matches("red"));
        Assert.False(machine.Matches(null!));
        Assert.False(machine.Matches(""));
        Assert.False(machine.Can("PEDESTRIAN_WAIT"));
        Assert.False(machine.Can(null!));
        Assert.False(machine.HasTag("stop"));
        Assert.False(machine.HasTag(null!));
        Assert.False(machine.IsDone);
        Assert.Equal("", machine.State);
        Assert.Equal("", machine.GetSnapshotJson());
        Assert.Equal("unstarted", machine.GetSnapshot().Status);
        Assert.False(machine.GetSnapshot().Matches("red"));
        Assert.Equal("", machine.GetOutput());
        Assert.Equal("", machine.GetMeta());
        Assert.Equal("", machine.GetStateMeta("red"));
        Assert.Equal("", machine.GetStateMeta(null!));
        Assert.Null(machine.NextDeadline);

        machine.Send("PEDESTRIAN_WAIT");
        machine.Send(null!);
        machine.Send(new { withoutType = 123 });
        machine.Tick();
        machine.Stop();
        machine.Close();
        machine.Dispose();
        Assert.False(machine.IsValid);
    }

    [Fact]
    public void FluentRegistration_OverloadedActionsAndGuards_TimeSpanDelays_AndValueInspection()
    {
        using var clock = new SimulatedClock(0);
        using var machine = new Machine(StoplightJson, clock: clock);

        var transitions = 0;
        string? seenLabel = null;
        bool? seenEnabled = null;
        int? seenParamStep = null;
        string? seenParamTag = null;
        int? lastEventPayload = null;

        machine
            .RegisterAction("onRed", (ctx, evt) =>
            {
                transitions++;
                seenLabel = ctx["label"].AsString;
                seenEnabled = ctx["enabled"].AsBool;
                Assert.True(ctx.IsObject);
                Assert.False(ctx.IsNull);
                Assert.True(ctx["nonexistent"].IsNull);
                Assert.True(ctx["nonexistent"]["deep"].IsNull);
                Assert.True(ctx["label"]["deep"].IsNull);
                Assert.True(ctx[null!].IsNull);
            })
            .RegisterAction("onGreen", (ctx, evt) => transitions++)
            .RegisterAction("onYellow", (ctx, evt) =>
            {
                transitions++;
                if (evt.IsObject && evt["urgency"].TryGetInt32(out var urgency))
                {
                    lastEventPayload = urgency;
                }
            })
            .RegisterAction("onParamAction", (ctx, evt, @params) =>
            {
                seenParamStep = @params["step"].AsInt32;
                seenParamTag = @params["tag"].AsString;
            })
            .RegisterGuard("canCross", (ctx, evt) => (ctx["cycles"].AsInt32 ?? 0) >= 1)
            .RegisterGuard("minCycles", (ctx, evt, @params) =>
                (ctx["cycles"].AsInt32 ?? 0) >= (@params["min"].AsInt32 ?? 999))
            .RegisterDelay("customDelay", (ctx, evt) => TimeSpan.FromMilliseconds(2500))
            .Start();

        Assert.True(machine.IsValid);
        Assert.True(machine.Matches("red"));
        Assert.True(machine.HasTag("stop"));
        Assert.False(machine.HasTag("go"));
        Assert.Equal(1, transitions);
        Assert.Equal("main-street", seenLabel);
        Assert.Equal(true, seenEnabled);
        Assert.Equal(2, seenParamStep);
        Assert.Equal("go-green", seenParamTag);
        Assert.Equal(1000UL, machine.NextDeadline);
        Assert.Contains("red", machine.GetMeta());
        Assert.Contains("1000", machine.GetStateMeta("red"));

        clock.Increment(TimeSpan.FromSeconds(1));
        Assert.Equal(1000UL, clock.Now);
        Assert.True(machine.Matches("green"));
        Assert.True(machine.HasTag("go"));
        Assert.Equal(2, transitions);
        Assert.True(machine.Can("PEDESTRIAN_WAIT"));
        Assert.True(machine.Can("PARAM_GUARD_EVENT"));
        Assert.Equal(3500UL, machine.NextDeadline);

        machine.SendJson("""{"type":"PEDESTRIAN_WAIT","urgency":42}""");
        Assert.True(machine.Matches("yellow"));
        Assert.Equal(3, transitions);
        Assert.Equal(42, lastEventPayload);

        clock.Set(2000);
        Assert.True(machine.Matches("red"));
        Assert.Equal(4, transitions);

        // Disposing machine detaches it from SimulatedClock so subsequent clock increments are safe
        machine.Dispose();
        clock.Increment(5000);
        Assert.Equal(4, transitions);
    }

    [Fact]
    public void StructuredSnapshot_Subscribe_RestoreSnapshot_AndFinalOutput()
    {
        using var clock1 = new SimulatedClock(0);
        using var machine1 = Machine.CreateActor(StoplightJson, clock: clock1)
            .RegisterDelay("customDelay", (ctx, evt) => 5000UL);

        var observedStates = new List<string>();
        using var sub = machine1.Subscribe(snap => observedStates.Add(snap.State));

        machine1.Start();
        clock1.Increment(1000);
        Assert.True(machine1.Matches("green"));

        var snap = machine1.GetSnapshot();
        Assert.Equal("active", snap.Status);
        Assert.Equal("green", snap.State);
        Assert.True(snap.Matches("green"));
        Assert.True(snap.HasTag("go"));
        Assert.Equal(1, snap.Context.GetProperty("cycles").GetInt32());
        Assert.Contains("green", observedStates);

        using var clock2 = new SimulatedClock(1000);
        using var machine2 = new Machine(StoplightJson, clock: clock2);
        var yellowEntered = false;
        machine2
            .RegisterDelay("customDelay", (ctx, evt) => 5000UL)
            .RegisterAction("onYellow", (ctx, evt) => yellowEntered = true)
            .RegisterGuardWithParams("minCycles", (ctx, evt, @params) =>
                (ctx["cycles"].AsInt64 ?? 0) >= (@params["min"].AsInt64 ?? 0));

        Assert.True(machine2.RestoreSnapshot(snap));
        Assert.True(machine2.Matches("green"));

        machine2.Send("PARAM_GUARD_EVENT");
        Assert.True(machine2.Matches("yellow"));
        Assert.True(yellowEntered);

        clock2.Increment(1000);
        Assert.True(machine2.Matches("red"));
        clock2.Increment(1000);
        Assert.True(machine2.Matches("green"));
        machine2.Send("FINISH");
        Assert.True(machine2.IsDone);
        Assert.Contains("completed", machine2.GetOutput());

        var doneSnap = machine2.GetSnapshot();
        Assert.Equal("done", doneSnap.Status);
        Assert.NotNull(doneSnap.Output);
        Assert.Equal("completed", doneSnap.Output.Value.GetProperty("result").GetString());
    }

    [Fact]
    public void CallbackReplacement_CustomClock_Tick_AndStop()
    {
        ulong nowMs = 100;
        using var machine = new Machine(StoplightJson);
        var actionHits = 0;
        machine
            .RegisterDelay("customDelay", (ctx, evt) => 5000UL)
            .RegisterAction("onRed", (ctx, evt) => actionHits += 10)
            .RegisterAction("onRed", (ctx, evt) => actionHits += 1)
            .RegisterClock(() => nowMs)
            .Start();

        Assert.Equal(1, actionHits);
        Assert.Equal(1100UL, machine.NextDeadline);

        nowMs = 1100;
        machine.Tick();
        Assert.True(machine.Matches("green"));

        machine.Stop();
        machine.Send("FINISH");
        Assert.False(machine.IsDone);
    }

    [Fact]
    public void InspectorCallback_AndInspectedEvent_ReceiveMicrostepAndSnapshotEvents()
    {
        using var clock = new SimulatedClock(0);
        using var machine = new Machine(StoplightJson, clock: clock);
        var callbackEvents = new List<string>();
        var csharpEventItems = new List<string>();

        machine.Inspected += json => csharpEventItems.Add(json);
        machine
            .RegisterDelay("customDelay", (ctx, evt) => 5000UL)
            .RegisterInspector(json => callbackEvents.Add(json))
            .Start();

        clock.Increment(1000);
        machine.Send("FINISH");
        Assert.True(machine.IsDone);
        Assert.Contains(callbackEvents, e => e.Contains("@xstate.microstep"));
        Assert.Contains(callbackEvents, e => e.Contains("@xstate.snapshot"));
        Assert.Contains(csharpEventItems, e => e.Contains("@xstate.snapshot"));
    }

    [Fact]
    public void AnonymousObjectInput_AndAnonymousObjectSendPayloads()
    {
        const string inputMachineJson = """
        {
          "id": "inputMachine",
          "initial": "idle",
          "context": ({ input }) => ({ "cycles": input.cycles }),
          "states": {
            "idle": {
              "on": {
                "STEP": {
                  "target": "active",
                  "actions": ["captureEvent"]
                }
              }
            },
            "active": {
              "on": {
                "OBJECT_EVENT": {
                  "target": "done",
                  "actions": ["captureEvent"]
                }
              }
            },
            "done": {
              "type": "final"
            }
          }
        }
        """;

        // Test anonymous object event payloads on standard JSON config + inputJson
        using var clock = new SimulatedClock(0);
        int? seenUrgency = null;
        string? seenNote = null;

        using var machine = new Machine(StoplightJson, input: new { cycles = 2 }, clock: clock)
            .RegisterDelay("customDelay", (ctx, evt) => 5000UL)
            .RegisterGuard("canCross", (ctx, evt) => (ctx["cycles"].AsInt32 ?? 0) >= 1)
            .RegisterAction("onYellow", (ctx, evt) =>
            {
                seenUrgency = evt["urgency"].AsInt32;
                seenNote = evt["note"].AsString;
            })
            .Start();

        clock.Increment(1000);
        Assert.True(machine.Matches("green"));

        machine.Send("PEDESTRIAN_WAIT", new { urgency = 7, note = "crosswalk" });
        Assert.True(machine.Matches("yellow"));
        Assert.Equal(7, seenUrgency);
        Assert.Equal("crosswalk", seenNote);

        clock.Increment(1000);
        clock.Increment(1000);
        Assert.True(machine.Matches("green"));

        // XState v5 style event object with `type` property
        machine.Send(new { type = "PEDESTRIAN_WAIT", urgency = 99, note = "object-style" });
        Assert.True(machine.Matches("yellow"));
        Assert.Equal(99, seenUrgency);
        Assert.Equal("object-style", seenNote);
        _ = inputMachineJson;
    }

    [Fact]
    public void PureTransitions_OutputResolver_FinalLeafOutput_AndInvokeCallbacks()
    {
        const string td21Json = """
        {
          "id": "td21Cs",
          "initial": "active",
          "context": { "total": 30 },
          "states": {
            "active": {
              "initial": "working",
              "invoke": { "id": "csWorker", "src": "csTask", "input": { "rate": 8 } },
              "states": {
                "working": {
                  "entry": ["logWorking"],
                  "on": { "DONE_LEAF": "leafFinal" }
                },
                "leafFinal": {
                  "type": "final",
                  "output": { "subStatus": "csLeafDone" }
                }
              },
              "on": { "FINISH_ALL": "completed" }
            },
            "completed": { "type": "final" }
          }
        }
        """;

        var startedInvokes = new List<string>();
        var stoppedInvokes = new List<string>();

        using var machine = new Machine(td21Json)
            .RegisterOutputResolver("", (ctx, evt) =>
            {
                var total = ctx["total"].AsInt32 ?? 0;
                return $$"""{"doubled":{{total * 2}}}""";
            })
            .RegisterInvokeCallbacks(
                onStart: (id, src, inp, ctx, evt) =>
                {
                    var rate = inp["rate"].AsInt32 ?? 0;
                    startedInvokes.Add($"{id}:{src}:{rate}");
                },
                onStop: id => stoppedInvokes.Add(id));

        var initPure = machine.InitialTransitionJson();
        Assert.Contains("\"value\":{\"active\":\"working\"}", initPure);
        Assert.Empty(startedInvokes);
        Assert.False(machine.IsValid);

        Assert.True(machine.TryStart());
        Assert.True(machine.IsValid);
        Assert.Equal(new[] { "csWorker:csTask:8" }, startedInvokes);

        var liveSnap = machine.GetSnapshotJson();
        var nextPure = machine.TransitionPureJson(liveSnap, """{"type":"FINISH_ALL"}""");
        Assert.Contains("\"value\":\"completed\"", nextPure);
        Assert.True(machine.Matches("active.working"));
        Assert.Empty(stoppedInvokes);

        Assert.Equal("", machine.GetFinalLeafOutput());
        machine.Send("DONE_LEAF");
        Assert.True(machine.Matches("active.leafFinal"));
        Assert.Contains("csLeafDone", machine.GetFinalLeafOutput());

        machine.Send("FINISH_ALL");
        Assert.True(machine.IsDone);
        Assert.Equal(new[] { "csWorker" }, stoppedInvokes);
        Assert.Contains("\"doubled\":60", machine.GetOutput());
    }
}
