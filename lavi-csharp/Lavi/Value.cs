using System;
using System.Diagnostics.CodeAnalysis;
using System.Text;

namespace Lavi;

/// <summary>
/// Read-only view of a Lavi JSON value (<c>context</c>, <c>event</c>, or <c>params</c>) passed into callbacks.
/// </summary>
public readonly struct Value
{
    private readonly IntPtr _ptr;

    internal Value(IntPtr ptr)
    {
        _ptr = ptr;
    }

    public static Value Null => new(IntPtr.Zero);

    public static Value FromRaw(IntPtr ptr) => new(ptr);

    public bool IsNull => _ptr == IntPtr.Zero;

    public bool IsObject => _ptr != IntPtr.Zero && NativeMethods.fsm_value_is_object(_ptr);

    public bool IsString => _ptr != IntPtr.Zero && NativeMethods.fsm_value_is_string(_ptr);

    public bool IsNumber => _ptr != IntPtr.Zero && NativeMethods.fsm_value_is_number(_ptr);

    public bool IsBool => _ptr != IntPtr.Zero && NativeMethods.fsm_value_is_bool(_ptr);

    public string? StringValue => TryGetString(out var s) ? s : null;

    public string? AsString => StringValue;

    public double? NumberValue => TryGetDouble(out var d) ? d : null;

    public double? AsDouble => NumberValue;

    public int? AsInt32 => TryGetInt32(out var i) ? i : null;

    public long? AsInt64 => TryGetInt64(out var l) ? l : null;

    public bool? BoolValue => TryGetBoolean(out var b) ? b : null;

    public bool? AsBool => BoolValue;

    public bool TryGetString([NotNullWhen(true)] out string? value)
    {
        if (!IsString)
        {
            value = null;
            return false;
        }

        var reqLen = (int)NativeMethods.fsm_value_get_string(_ptr, null, UIntPtr.Zero);
        if (reqLen == 0)
        {
            value = string.Empty;
            return true;
        }

        var buf = new byte[reqLen + 1];
        var copied = (int)NativeMethods.fsm_value_get_string(_ptr, buf, (UIntPtr)buf.Length);
        if (copied <= 0)
        {
            value = null;
            return false;
        }

        value = Encoding.UTF8.GetString(buf, 0, copied);
        return true;
    }

    public bool TryGetDouble(out double value)
    {
        if (!IsNumber)
        {
            value = default;
            return false;
        }

        value = NativeMethods.fsm_value_get_number(_ptr);
        return true;
    }

    public bool TryGetInt32(out int value)
    {
        if (TryGetDouble(out var d) && !double.IsNaN(d) && !double.IsInfinity(d) &&
            d >= int.MinValue && d <= int.MaxValue)
        {
            value = (int)d;
            return true;
        }

        value = default;
        return false;
    }

    public bool TryGetInt64(out long value)
    {
        if (TryGetDouble(out var d) && !double.IsNaN(d) && !double.IsInfinity(d) &&
            d >= long.MinValue && d <= long.MaxValue)
        {
            value = (long)d;
            return true;
        }

        value = default;
        return false;
    }

    public bool TryGetBoolean(out bool value)
    {
        if (!IsBool)
        {
            value = default;
            return false;
        }

        value = NativeMethods.fsm_value_get_bool(_ptr);
        return true;
    }

    public Value Property(string? key)
    {
        if (_ptr == IntPtr.Zero || string.IsNullOrEmpty(key))
        {
            return Null;
        }

        var propPtr = NativeMethods.fsm_value_get_property(_ptr, key);
        return new Value(propPtr);
    }

    public Value this[string? key] => Property(key);
}
