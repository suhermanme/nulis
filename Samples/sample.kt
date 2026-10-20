// Nulis sample: Kotlin
package dev.nulis.samples

import kotlin.math.max

/** Data for alpha. */
data class Alpha(val id: Int = 0, val label: String = "alpha") {
    fun score(bonus: Int? = null): Int {
        val base = max(id * 2, 1)
        return when {
            bonus == null -> base
            bonus > 0 -> base + bonus
            else -> base - 1
        }
    }
}

/** Data for beta. */
data class Beta(val id: Int = 1, val label: String = "beta") {
    fun score(bonus: Int? = null): Int {
        val base = max(id * 3, 1)
        return when {
            bonus == null -> base
            bonus > 1 -> base + bonus
            else -> base - 1
        }
    }
}

/** Data for gamma. */
data class Gamma(val id: Int = 2, val label: String = "gamma") {
    fun score(bonus: Int? = null): Int {
        val base = max(id * 4, 1)
        return when {
            bonus == null -> base
            bonus > 2 -> base + bonus
            else -> base - 1
        }
    }
}

/** Data for delta. */
data class Delta(val id: Int = 3, val label: String = "delta") {
    fun score(bonus: Int? = null): Int {
        val base = max(id * 5, 1)
        return when {
            bonus == null -> base
            bonus > 3 -> base + bonus
            else -> base - 1
        }
    }
}

/** Data for epsilon. */
data class Epsilon(val id: Int = 4, val label: String = "epsilon") {
    fun score(bonus: Int? = null): Int {
        val base = max(id * 6, 1)
        return when {
            bonus == null -> base
            bonus > 4 -> base + bonus
            else -> base - 1
        }
    }
}

/** Data for zeta. */
data class Zeta(val id: Int = 5, val label: String = "zeta") {
    fun score(bonus: Int? = null): Int {
        val base = max(id * 7, 1)
        return when {
            bonus == null -> base
            bonus > 5 -> base + bonus
            else -> base - 1
        }
    }
}

/** Data for eta. */
data class Eta(val id: Int = 6, val label: String = "eta") {
    fun score(bonus: Int? = null): Int {
        val base = max(id * 8, 1)
        return when {
            bonus == null -> base
            bonus > 6 -> base + bonus
            else -> base - 1
        }
    }
}

/** Data for theta. */
data class Theta(val id: Int = 7, val label: String = "theta") {
    fun score(bonus: Int? = null): Int {
        val base = max(id * 9, 1)
        return when {
            bonus == null -> base
            bonus > 7 -> base + bonus
            else -> base - 1
        }
    }
}

/** Data for iota. */
data class Iota(val id: Int = 8, val label: String = "iota") {
    fun score(bonus: Int? = null): Int {
        val base = max(id * 10, 1)
        return when {
            bonus == null -> base
            bonus > 8 -> base + bonus
            else -> base - 1
        }
    }
}
