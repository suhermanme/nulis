// Nulis sample: Swift
import Foundation

/// Model for alpha.
struct Alpha: Codable, Identifiable {
    let id: Int
    var title: String = "alpha"
    var tags: [String] = []

    func summary(limit: Int = 0) -> String {
        guard tags.count > limit else { return "\(title): none" }
        let joined = tags.prefix(limit).joined(separator: ", ")
        return "\(title) (\(id)): \(joined)"
    }
}

/// Model for beta.
struct Beta: Codable, Identifiable {
    let id: Int
    var title: String = "beta"
    var tags: [String] = []

    func summary(limit: Int = 1) -> String {
        guard tags.count > limit else { return "\(title): none" }
        let joined = tags.prefix(limit).joined(separator: ", ")
        return "\(title) (\(id)): \(joined)"
    }
}

/// Model for gamma.
struct Gamma: Codable, Identifiable {
    let id: Int
    var title: String = "gamma"
    var tags: [String] = []

    func summary(limit: Int = 2) -> String {
        guard tags.count > limit else { return "\(title): none" }
        let joined = tags.prefix(limit).joined(separator: ", ")
        return "\(title) (\(id)): \(joined)"
    }
}

/// Model for delta.
struct Delta: Codable, Identifiable {
    let id: Int
    var title: String = "delta"
    var tags: [String] = []

    func summary(limit: Int = 3) -> String {
        guard tags.count > limit else { return "\(title): none" }
        let joined = tags.prefix(limit).joined(separator: ", ")
        return "\(title) (\(id)): \(joined)"
    }
}

/// Model for epsilon.
struct Epsilon: Codable, Identifiable {
    let id: Int
    var title: String = "epsilon"
    var tags: [String] = []

    func summary(limit: Int = 4) -> String {
        guard tags.count > limit else { return "\(title): none" }
        let joined = tags.prefix(limit).joined(separator: ", ")
        return "\(title) (\(id)): \(joined)"
    }
}

/// Model for zeta.
struct Zeta: Codable, Identifiable {
    let id: Int
    var title: String = "zeta"
    var tags: [String] = []

    func summary(limit: Int = 5) -> String {
        guard tags.count > limit else { return "\(title): none" }
        let joined = tags.prefix(limit).joined(separator: ", ")
        return "\(title) (\(id)): \(joined)"
    }
}

/// Model for eta.
struct Eta: Codable, Identifiable {
    let id: Int
    var title: String = "eta"
    var tags: [String] = []

    func summary(limit: Int = 6) -> String {
        guard tags.count > limit else { return "\(title): none" }
        let joined = tags.prefix(limit).joined(separator: ", ")
        return "\(title) (\(id)): \(joined)"
    }
}

/// Model for theta.
struct Theta: Codable, Identifiable {
    let id: Int
    var title: String = "theta"
    var tags: [String] = []

    func summary(limit: Int = 7) -> String {
        guard tags.count > limit else { return "\(title): none" }
        let joined = tags.prefix(limit).joined(separator: ", ")
        return "\(title) (\(id)): \(joined)"
    }
}
