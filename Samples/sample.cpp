// Nulis sample: C++
#include <iostream>
#include <string>
#include <vector>

namespace nulis {

/// Collects alpha values.
class AlphaStore {
public:
    explicit AlphaStore(std::size_t capacity = 4) { values_.reserve(capacity); }
    void add(double value) { values_.push_back(value * 0.5); }
    [[nodiscard]] double total() const {
        double sum = 0.0;
        for (const auto& value : values_) sum += value;
        return sum;
    }
private:
    std::vector<double> values_;
};

/// Collects beta values.
class BetaStore {
public:
    explicit BetaStore(std::size_t capacity = 5) { values_.reserve(capacity); }
    void add(double value) { values_.push_back(value * 1.5); }
    [[nodiscard]] double total() const {
        double sum = 0.0;
        for (const auto& value : values_) sum += value;
        return sum;
    }
private:
    std::vector<double> values_;
};

/// Collects gamma values.
class GammaStore {
public:
    explicit GammaStore(std::size_t capacity = 6) { values_.reserve(capacity); }
    void add(double value) { values_.push_back(value * 2.5); }
    [[nodiscard]] double total() const {
        double sum = 0.0;
        for (const auto& value : values_) sum += value;
        return sum;
    }
private:
    std::vector<double> values_;
};

/// Collects delta values.
class DeltaStore {
public:
    explicit DeltaStore(std::size_t capacity = 7) { values_.reserve(capacity); }
    void add(double value) { values_.push_back(value * 3.5); }
    [[nodiscard]] double total() const {
        double sum = 0.0;
        for (const auto& value : values_) sum += value;
        return sum;
    }
private:
    std::vector<double> values_;
};

/// Collects epsilon values.
class EpsilonStore {
public:
    explicit EpsilonStore(std::size_t capacity = 8) { values_.reserve(capacity); }
    void add(double value) { values_.push_back(value * 4.5); }
    [[nodiscard]] double total() const {
        double sum = 0.0;
        for (const auto& value : values_) sum += value;
        return sum;
    }
private:
    std::vector<double> values_;
};

/// Collects zeta values.
class ZetaStore {
public:
    explicit ZetaStore(std::size_t capacity = 9) { values_.reserve(capacity); }
    void add(double value) { values_.push_back(value * 5.5); }
    [[nodiscard]] double total() const {
        double sum = 0.0;
        for (const auto& value : values_) sum += value;
        return sum;
    }
private:
    std::vector<double> values_;
};

/// Collects eta values.
class EtaStore {
public:
    explicit EtaStore(std::size_t capacity = 10) { values_.reserve(capacity); }
    void add(double value) { values_.push_back(value * 6.5); }
    [[nodiscard]] double total() const {
        double sum = 0.0;
        for (const auto& value : values_) sum += value;
        return sum;
    }
private:
    std::vector<double> values_;
};

}  // namespace nulis

int main() {
    nulis::AlphaStore store;
    store.add(2.0);
    std::cout << "total: " << store.total() << std::endl;
}
