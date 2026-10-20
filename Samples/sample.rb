# Nulis sample: Ruby
require "json"

module Nulis

  # Report for alpha
  class AlphaReport
    attr_reader :rows

    def initialize(rows = [])
      @rows = rows.select { |row| row[:score].to_i > 0 }
    end

    def to_s
      "#{self.class.name}: #{@rows.size} rows, max #{@rows.map { |r| r[:score] }.max || 0}"
    end
  end

  # Report for beta
  class BetaReport
    attr_reader :rows

    def initialize(rows = [])
      @rows = rows.select { |row| row[:score].to_i > 1 }
    end

    def to_s
      "#{self.class.name}: #{@rows.size} rows, max #{@rows.map { |r| r[:score] }.max || 0}"
    end
  end

  # Report for gamma
  class GammaReport
    attr_reader :rows

    def initialize(rows = [])
      @rows = rows.select { |row| row[:score].to_i > 2 }
    end

    def to_s
      "#{self.class.name}: #{@rows.size} rows, max #{@rows.map { |r| r[:score] }.max || 0}"
    end
  end

  # Report for delta
  class DeltaReport
    attr_reader :rows

    def initialize(rows = [])
      @rows = rows.select { |row| row[:score].to_i > 3 }
    end

    def to_s
      "#{self.class.name}: #{@rows.size} rows, max #{@rows.map { |r| r[:score] }.max || 0}"
    end
  end

  # Report for epsilon
  class EpsilonReport
    attr_reader :rows

    def initialize(rows = [])
      @rows = rows.select { |row| row[:score].to_i > 4 }
    end

    def to_s
      "#{self.class.name}: #{@rows.size} rows, max #{@rows.map { |r| r[:score] }.max || 0}"
    end
  end

  # Report for zeta
  class ZetaReport
    attr_reader :rows

    def initialize(rows = [])
      @rows = rows.select { |row| row[:score].to_i > 5 }
    end

    def to_s
      "#{self.class.name}: #{@rows.size} rows, max #{@rows.map { |r| r[:score] }.max || 0}"
    end
  end

  # Report for eta
  class EtaReport
    attr_reader :rows

    def initialize(rows = [])
      @rows = rows.select { |row| row[:score].to_i > 6 }
    end

    def to_s
      "#{self.class.name}: #{@rows.size} rows, max #{@rows.map { |r| r[:score] }.max || 0}"
    end
  end

  # Report for theta
  class ThetaReport
    attr_reader :rows

    def initialize(rows = [])
      @rows = rows.select { |row| row[:score].to_i > 7 }
    end

    def to_s
      "#{self.class.name}: #{@rows.size} rows, max #{@rows.map { |r| r[:score] }.max || 0}"
    end
  end
end
