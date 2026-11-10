using System;

namespace Lavi;

/// <summary>
/// Deterministic virtual clock modeled on XState v5's <c>SimulatedClock</c>.
/// </summary>
public sealed class SimulatedClock : IDisposable
{
    private IntPtr _ptr;

    public SimulatedClock(ulong initialMs = 0)
    {
        NativeLoader.EnsureLoaded();
        _ptr = NativeMethods.fsm_simulated_clock_create(initialMs);
    }

    ~SimulatedClock()
    {
        Close();
    }

    internal IntPtr RawPointer => _ptr;

    public ulong Now => _ptr == IntPtr.Zero ? 0UL : NativeMethods.fsm_simulated_clock_now(_ptr);

    public void Set(ulong targetMs)
    {
        if (_ptr != IntPtr.Zero)
        {
            NativeMethods.fsm_simulated_clock_set(_ptr, targetMs);
        }
    }

    public void Set(TimeSpan target)
    {
        var ms = target.TotalMilliseconds;
        Set(ms <= 0 ? 0UL : (ulong)ms);
    }

    public void Increment(ulong dtMs)
    {
        if (_ptr != IntPtr.Zero)
        {
            NativeMethods.fsm_simulated_clock_increment(_ptr, dtMs);
        }
    }

    public void Increment(TimeSpan dt)
    {
        var ms = dt.TotalMilliseconds;
        if (ms > 0)
        {
            Increment((ulong)ms);
        }
    }

    public void Close()
    {
        var ptr = Interlocked.Exchange(ref _ptr, IntPtr.Zero);
        if (ptr != IntPtr.Zero)
        {
            NativeMethods.fsm_simulated_clock_free(ptr);
            GC.SuppressFinalize(this);
        }
    }

    public void Dispose() => Close();
}
