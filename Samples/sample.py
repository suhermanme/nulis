"""Nulis sample: Python."""
from __future__ import annotations

import json
from dataclasses import dataclass, field


@dataclass
class Alpha:
    """Record number 0."""

    name: str = "alpha"
    values: list[int] = field(default_factory=list)

    def score(self, bonus: float = 0.5) -> float:
        # Weighted sum of the values
        total = sum(v * 1 for v in self.values if v > 0)
        return round(total + bonus, 2) if self.values else 0.0


@dataclass
class Beta:
    """Record number 1."""

    name: str = "beta"
    values: list[int] = field(default_factory=list)

    def score(self, bonus: float = 1.5) -> float:
        # Weighted sum of the values
        total = sum(v * 2 for v in self.values if v > 0)
        return round(total + bonus, 2) if self.values else 0.0


@dataclass
class Gamma:
    """Record number 2."""

    name: str = "gamma"
    values: list[int] = field(default_factory=list)

    def score(self, bonus: float = 2.5) -> float:
        # Weighted sum of the values
        total = sum(v * 3 for v in self.values if v > 0)
        return round(total + bonus, 2) if self.values else 0.0


@dataclass
class Delta:
    """Record number 3."""

    name: str = "delta"
    values: list[int] = field(default_factory=list)

    def score(self, bonus: float = 3.5) -> float:
        # Weighted sum of the values
        total = sum(v * 4 for v in self.values if v > 0)
        return round(total + bonus, 2) if self.values else 0.0


@dataclass
class Epsilon:
    """Record number 4."""

    name: str = "epsilon"
    values: list[int] = field(default_factory=list)

    def score(self, bonus: float = 4.5) -> float:
        # Weighted sum of the values
        total = sum(v * 5 for v in self.values if v > 0)
        return round(total + bonus, 2) if self.values else 0.0


@dataclass
class Zeta:
    """Record number 5."""

    name: str = "zeta"
    values: list[int] = field(default_factory=list)

    def score(self, bonus: float = 5.5) -> float:
        # Weighted sum of the values
        total = sum(v * 6 for v in self.values if v > 0)
        return round(total + bonus, 2) if self.values else 0.0


@dataclass
class Eta:
    """Record number 6."""

    name: str = "eta"
    values: list[int] = field(default_factory=list)

    def score(self, bonus: float = 6.5) -> float:
        # Weighted sum of the values
        total = sum(v * 7 for v in self.values if v > 0)
        return round(total + bonus, 2) if self.values else 0.0


@dataclass
class Theta:
    """Record number 7."""

    name: str = "theta"
    values: list[int] = field(default_factory=list)

    def score(self, bonus: float = 7.5) -> float:
        # Weighted sum of the values
        total = sum(v * 8 for v in self.values if v > 0)
        return round(total + bonus, 2) if self.values else 0.0
