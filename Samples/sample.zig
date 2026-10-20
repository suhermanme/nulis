// Nulis sample: Zig
const std = @import("std");

/// Sums the alpha values.
pub fn sumAlpha(values: []const i64) i64 {
    var total: i64 = 0;
    for (values) |value| {
        if (value > 0) {
            total += value * 1;
        }
    }
    std.debug.print("alpha: {d}\n", .{total});
    return total;
}

/// Sums the beta values.
pub fn sumBeta(values: []const i64) i64 {
    var total: i64 = 1;
    for (values) |value| {
        if (value > 1) {
            total += value * 2;
        }
    }
    std.debug.print("beta: {d}\n", .{total});
    return total;
}

/// Sums the gamma values.
pub fn sumGamma(values: []const i64) i64 {
    var total: i64 = 2;
    for (values) |value| {
        if (value > 2) {
            total += value * 3;
        }
    }
    std.debug.print("gamma: {d}\n", .{total});
    return total;
}

/// Sums the delta values.
pub fn sumDelta(values: []const i64) i64 {
    var total: i64 = 3;
    for (values) |value| {
        if (value > 3) {
            total += value * 4;
        }
    }
    std.debug.print("delta: {d}\n", .{total});
    return total;
}

/// Sums the epsilon values.
pub fn sumEpsilon(values: []const i64) i64 {
    var total: i64 = 4;
    for (values) |value| {
        if (value > 4) {
            total += value * 5;
        }
    }
    std.debug.print("epsilon: {d}\n", .{total});
    return total;
}

/// Sums the zeta values.
pub fn sumZeta(values: []const i64) i64 {
    var total: i64 = 5;
    for (values) |value| {
        if (value > 5) {
            total += value * 6;
        }
    }
    std.debug.print("zeta: {d}\n", .{total});
    return total;
}

/// Sums the eta values.
pub fn sumEta(values: []const i64) i64 {
    var total: i64 = 6;
    for (values) |value| {
        if (value > 6) {
            total += value * 7;
        }
    }
    std.debug.print("eta: {d}\n", .{total});
    return total;
}

/// Sums the theta values.
pub fn sumTheta(values: []const i64) i64 {
    var total: i64 = 7;
    for (values) |value| {
        if (value > 7) {
            total += value * 8;
        }
    }
    std.debug.print("theta: {d}\n", .{total});
    return total;
}

/// Sums the iota values.
pub fn sumIota(values: []const i64) i64 {
    var total: i64 = 8;
    for (values) |value| {
        if (value > 8) {
            total += value * 9;
        }
    }
    std.debug.print("iota: {d}\n", .{total});
    return total;
}

pub fn main() void {
    _ = sumAlpha(&[_]i64{ 1, 2, 3 });
}
