using System;
using Xunit;

namespace Lavi.Tests;

public class ClockAndValueTests
{
    [Fact]
    public void SimulatedClock_TracksTimeAndDisposesIdempotently()
    {
        using var clock = new SimulatedClock(100);
        Assert.Equal(100UL, clock.Now);

        clock.Increment(250);
        Assert.Equal(350UL, clock.Now);

        clock.Increment(TimeSpan.FromMilliseconds(150));
        Assert.Equal(500UL, clock.Now);

        clock.Set(TimeSpan.FromSeconds(2));
        Assert.Equal(2000UL, clock.Now);

        clock.Close();
        clock.Dispose();
        Assert.Equal(0UL, clock.Now);
    }

    [Fact]
    public void Value_Null_ReturnsSafeDefaults()
    {
        var v = Value.Null;
        Assert.True(v.IsNull);
        Assert.False(v.IsObject);
        Assert.False(v.IsString);
        Assert.False(v.IsNumber);
        Assert.False(v.IsBool);
        Assert.Null(v.AsString);
        Assert.Null(v.AsDouble);
        Assert.Null(v.AsInt32);
        Assert.Null(v.AsInt64);
        Assert.Null(v.AsBool);
        Assert.False(v.TryGetInt32(out _));
        Assert.True(v["missing"]["deep"].IsNull);
    }
}
