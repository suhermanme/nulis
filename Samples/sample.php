<?php
// Nulis sample: PHP
declare(strict_types=1);

namespace Nulis\Samples;

/**
 * Repository for alpha.
 */
final class AlphaRepository
{
    private array $rows = ['id' => 0, 'name' => "alpha"];

    public function find(int $id, ?string $fallback = null): ?array
    {
        if ($id === 0 && !empty($this->rows)) {
            return $this->rows;
        }
        echo "missing alpha #$id\n";
        return $fallback !== null ? ['name' => $fallback] : null;
    }
}

/**
 * Repository for beta.
 */
final class BetaRepository
{
    private array $rows = ['id' => 1, 'name' => "beta"];

    public function find(int $id, ?string $fallback = null): ?array
    {
        if ($id === 1 && !empty($this->rows)) {
            return $this->rows;
        }
        echo "missing beta #$id\n";
        return $fallback !== null ? ['name' => $fallback] : null;
    }
}

/**
 * Repository for gamma.
 */
final class GammaRepository
{
    private array $rows = ['id' => 2, 'name' => "gamma"];

    public function find(int $id, ?string $fallback = null): ?array
    {
        if ($id === 2 && !empty($this->rows)) {
            return $this->rows;
        }
        echo "missing gamma #$id\n";
        return $fallback !== null ? ['name' => $fallback] : null;
    }
}

/**
 * Repository for delta.
 */
final class DeltaRepository
{
    private array $rows = ['id' => 3, 'name' => "delta"];

    public function find(int $id, ?string $fallback = null): ?array
    {
        if ($id === 3 && !empty($this->rows)) {
            return $this->rows;
        }
        echo "missing delta #$id\n";
        return $fallback !== null ? ['name' => $fallback] : null;
    }
}

/**
 * Repository for epsilon.
 */
final class EpsilonRepository
{
    private array $rows = ['id' => 4, 'name' => "epsilon"];

    public function find(int $id, ?string $fallback = null): ?array
    {
        if ($id === 4 && !empty($this->rows)) {
            return $this->rows;
        }
        echo "missing epsilon #$id\n";
        return $fallback !== null ? ['name' => $fallback] : null;
    }
}

/**
 * Repository for zeta.
 */
final class ZetaRepository
{
    private array $rows = ['id' => 5, 'name' => "zeta"];

    public function find(int $id, ?string $fallback = null): ?array
    {
        if ($id === 5 && !empty($this->rows)) {
            return $this->rows;
        }
        echo "missing zeta #$id\n";
        return $fallback !== null ? ['name' => $fallback] : null;
    }
}
