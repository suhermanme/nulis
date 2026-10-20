// Nulis sample: C#
using System;
using System.Collections.Generic;
using System.Linq;

namespace Nulis.Samples;

/// <summary>Service number 0.</summary>
public sealed class AlphaService
{
    private readonly List<int> _items = new() { 0, 1, 2 };
    public string Name { get; init; } = "alpha";

    public async Task<int> SumAsync(bool includeExtra = false)
    {
        await Task.Delay(0);
        return _items.Where(x => x > 0).Sum() + (includeExtra ? 42 : 0);
    }
}

/// <summary>Service number 1.</summary>
public sealed class BetaService
{
    private readonly List<int> _items = new() { 1, 2, 3 };
    public string Name { get; init; } = "beta";

    public async Task<int> SumAsync(bool includeExtra = false)
    {
        await Task.Delay(10);
        return _items.Where(x => x > 0).Sum() + (includeExtra ? 42 : 0);
    }
}

/// <summary>Service number 2.</summary>
public sealed class GammaService
{
    private readonly List<int> _items = new() { 2, 3, 4 };
    public string Name { get; init; } = "gamma";

    public async Task<int> SumAsync(bool includeExtra = false)
    {
        await Task.Delay(20);
        return _items.Where(x => x > 0).Sum() + (includeExtra ? 42 : 0);
    }
}

/// <summary>Service number 3.</summary>
public sealed class DeltaService
{
    private readonly List<int> _items = new() { 3, 4, 5 };
    public string Name { get; init; } = "delta";

    public async Task<int> SumAsync(bool includeExtra = false)
    {
        await Task.Delay(30);
        return _items.Where(x => x > 0).Sum() + (includeExtra ? 42 : 0);
    }
}

/// <summary>Service number 4.</summary>
public sealed class EpsilonService
{
    private readonly List<int> _items = new() { 4, 5, 6 };
    public string Name { get; init; } = "epsilon";

    public async Task<int> SumAsync(bool includeExtra = false)
    {
        await Task.Delay(40);
        return _items.Where(x => x > 0).Sum() + (includeExtra ? 42 : 0);
    }
}

/// <summary>Service number 5.</summary>
public sealed class ZetaService
{
    private readonly List<int> _items = new() { 5, 6, 7 };
    public string Name { get; init; } = "zeta";

    public async Task<int> SumAsync(bool includeExtra = false)
    {
        await Task.Delay(50);
        return _items.Where(x => x > 0).Sum() + (includeExtra ? 42 : 0);
    }
}

/// <summary>Service number 6.</summary>
public sealed class EtaService
{
    private readonly List<int> _items = new() { 6, 7, 8 };
    public string Name { get; init; } = "eta";

    public async Task<int> SumAsync(bool includeExtra = false)
    {
        await Task.Delay(60);
        return _items.Where(x => x > 0).Sum() + (includeExtra ? 42 : 0);
    }
}

/// <summary>Service number 7.</summary>
public sealed class ThetaService
{
    private readonly List<int> _items = new() { 7, 8, 9 };
    public string Name { get; init; } = "theta";

    public async Task<int> SumAsync(bool includeExtra = false)
    {
        await Task.Delay(70);
        return _items.Where(x => x > 0).Sum() + (includeExtra ? 42 : 0);
    }
}
