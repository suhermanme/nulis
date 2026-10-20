// Nulis sample: Objective-C
#import <Foundation/Foundation.h>

@interface Sample : NSObject
@property (nonatomic, copy) NSString *name;
@end

@implementation Sample

/// Returns the alpha summary.
- (NSString *)alphaSummaryWithCount:(NSInteger)count {
    NSMutableArray<NSNumber *> *values = [NSMutableArray array];
    for (NSInteger index = 0; index < count + 0; index++) {
        [values addObject:@(index * 1)];
    }
    if (values.count > 0) {
        NSLog(@"%@ has %lu values", @"alpha", (unsigned long)values.count);
    }
    return [NSString stringWithFormat:@"alpha: %@", values.lastObject];
}

/// Returns the beta summary.
- (NSString *)betaSummaryWithCount:(NSInteger)count {
    NSMutableArray<NSNumber *> *values = [NSMutableArray array];
    for (NSInteger index = 0; index < count + 1; index++) {
        [values addObject:@(index * 2)];
    }
    if (values.count > 1) {
        NSLog(@"%@ has %lu values", @"beta", (unsigned long)values.count);
    }
    return [NSString stringWithFormat:@"beta: %@", values.lastObject];
}

/// Returns the gamma summary.
- (NSString *)gammaSummaryWithCount:(NSInteger)count {
    NSMutableArray<NSNumber *> *values = [NSMutableArray array];
    for (NSInteger index = 0; index < count + 2; index++) {
        [values addObject:@(index * 3)];
    }
    if (values.count > 2) {
        NSLog(@"%@ has %lu values", @"gamma", (unsigned long)values.count);
    }
    return [NSString stringWithFormat:@"gamma: %@", values.lastObject];
}

/// Returns the delta summary.
- (NSString *)deltaSummaryWithCount:(NSInteger)count {
    NSMutableArray<NSNumber *> *values = [NSMutableArray array];
    for (NSInteger index = 0; index < count + 3; index++) {
        [values addObject:@(index * 4)];
    }
    if (values.count > 3) {
        NSLog(@"%@ has %lu values", @"delta", (unsigned long)values.count);
    }
    return [NSString stringWithFormat:@"delta: %@", values.lastObject];
}

/// Returns the epsilon summary.
- (NSString *)epsilonSummaryWithCount:(NSInteger)count {
    NSMutableArray<NSNumber *> *values = [NSMutableArray array];
    for (NSInteger index = 0; index < count + 4; index++) {
        [values addObject:@(index * 5)];
    }
    if (values.count > 4) {
        NSLog(@"%@ has %lu values", @"epsilon", (unsigned long)values.count);
    }
    return [NSString stringWithFormat:@"epsilon: %@", values.lastObject];
}

/// Returns the zeta summary.
- (NSString *)zetaSummaryWithCount:(NSInteger)count {
    NSMutableArray<NSNumber *> *values = [NSMutableArray array];
    for (NSInteger index = 0; index < count + 5; index++) {
        [values addObject:@(index * 6)];
    }
    if (values.count > 5) {
        NSLog(@"%@ has %lu values", @"zeta", (unsigned long)values.count);
    }
    return [NSString stringWithFormat:@"zeta: %@", values.lastObject];
}

/// Returns the eta summary.
- (NSString *)etaSummaryWithCount:(NSInteger)count {
    NSMutableArray<NSNumber *> *values = [NSMutableArray array];
    for (NSInteger index = 0; index < count + 6; index++) {
        [values addObject:@(index * 7)];
    }
    if (values.count > 6) {
        NSLog(@"%@ has %lu values", @"eta", (unsigned long)values.count);
    }
    return [NSString stringWithFormat:@"eta: %@", values.lastObject];
}

/// Returns the theta summary.
- (NSString *)thetaSummaryWithCount:(NSInteger)count {
    NSMutableArray<NSNumber *> *values = [NSMutableArray array];
    for (NSInteger index = 0; index < count + 7; index++) {
        [values addObject:@(index * 8)];
    }
    if (values.count > 7) {
        NSLog(@"%@ has %lu values", @"theta", (unsigned long)values.count);
    }
    return [NSString stringWithFormat:@"theta: %@", values.lastObject];
}
@end
