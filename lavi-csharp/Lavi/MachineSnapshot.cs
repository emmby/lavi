using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;

namespace Lavi;

/// <summary>
/// Structured XState v5 snapshot of a <see cref="Machine"/> at a point in time.
/// </summary>
public sealed class MachineSnapshot
{
    private static readonly JsonElement EmptyObject = ParseElement("{}");
    private static readonly JsonElement UndefinedElement = default;

    private readonly string _rawJson;
    private readonly WeakReference<Machine>? _machineRef;

    internal static MachineSnapshot Unstarted { get; } = new(
        rawJson: string.Empty,
        status: "unstarted",
        state: string.Empty,
        stateValue: UndefinedElement,
        context: EmptyObject,
        output: null,
        tags: Array.Empty<string>(),
        machine: null);

    private MachineSnapshot(
        string rawJson,
        string status,
        string state,
        JsonElement stateValue,
        JsonElement context,
        JsonElement? output,
        IReadOnlyList<string> tags,
        Machine? machine)
    {
        _rawJson = rawJson;
        Status = status;
        State = state;
        StateValue = stateValue;
        Context = context;
        Output = output;
        Tags = tags;
        _machineRef = machine != null ? new WeakReference<Machine>(machine) : null;
    }

    public string Status { get; }

    public string State { get; }

    public JsonElement StateValue { get; }

    public JsonElement Context { get; }

    public JsonElement? Output { get; }

    public IReadOnlyList<string> Tags { get; }

    internal static MachineSnapshot FromJson(string rawJson, string stateStr, string outputJson, Machine? machine)
    {
        if (string.IsNullOrWhiteSpace(rawJson))
        {
            return Unstarted;
        }

        using var doc = JsonDocument.Parse(rawJson);
        var root = doc.RootElement;

        var status = root.TryGetProperty("status", out var statusEl) && statusEl.ValueKind == JsonValueKind.String
            ? statusEl.GetString() ?? "active"
            : "active";

        var stateValue = root.TryGetProperty("value", out var valEl)
            ? valEl.Clone()
            : UndefinedElement;

        var resolvedState = stateValue.ValueKind == JsonValueKind.String
            ? stateValue.GetString() ?? string.Empty
            : stateValue.ValueKind == JsonValueKind.Undefined
                ? stateStr
                : stateValue.GetRawText();

        var context = root.TryGetProperty("context", out var ctxEl)
            ? ctxEl.Clone()
            : EmptyObject;

        var tags = new List<string>();
        if (root.TryGetProperty("tags", out var tagsEl) && tagsEl.ValueKind == JsonValueKind.Array)
        {
            foreach (var item in tagsEl.EnumerateArray())
            {
                if (item.ValueKind == JsonValueKind.String && item.GetString() is { } s)
                {
                    tags.Add(s);
                }
            }
        }

        JsonElement? output = null;
        if (!string.IsNullOrWhiteSpace(outputJson))
        {
            output = ParseElement(outputJson);
        }

        return new MachineSnapshot(
            rawJson: rawJson,
            status: status,
            state: resolvedState,
            stateValue: stateValue,
            context: context,
            output: output,
            tags: tags,
            machine: machine);
    }

    public bool Matches(string? statePath)
    {
        if (string.IsNullOrEmpty(statePath) || StateValue.ValueKind == JsonValueKind.Undefined)
        {
            return false;
        }

        var parts = statePath.Split('.', StringSplitOptions.RemoveEmptyEntries);
        if (parts.Length == 0)
        {
            return false;
        }

        return MatchesElement(StateValue, parts, 0);
    }

    private static bool MatchesElement(JsonElement element, string[] parts, int index)
    {
        if (index >= parts.Length)
        {
            return true;
        }

        var current = parts[index];
        if (element.ValueKind == JsonValueKind.String)
        {
            return index == parts.Length - 1 && element.GetString() == current;
        }

        if (element.ValueKind == JsonValueKind.Object)
        {
            if (!element.TryGetProperty(current, out var child))
            {
                return false;
            }
            return MatchesElement(child, parts, index + 1);
        }

        return false;
    }

    public bool Can(string? eventName)
    {
        if (string.IsNullOrEmpty(eventName))
        {
            return false;
        }

        if (_machineRef != null && _machineRef.TryGetTarget(out var machine) && machine.IsValid)
        {
            return machine.Can(eventName);
        }

        return false;
    }

    public bool HasTag(string? tag)
    {
        if (string.IsNullOrEmpty(tag))
        {
            return false;
        }

        return Tags.Contains(tag, StringComparer.Ordinal);
    }

    public string ToJson() => _rawJson;

    public override string ToString() => _rawJson;

    public static implicit operator string(MachineSnapshot? snapshot) => snapshot?._rawJson ?? string.Empty;

    private static JsonElement ParseElement(string json)
    {
        using var doc = JsonDocument.Parse(json);
        return doc.RootElement.Clone();
    }
}
