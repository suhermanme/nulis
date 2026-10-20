#!/usr/bin/env bash
# Nulis sample: Bash
set -euo pipefail
LOG_DIR="${HOME}/logs"
mkdir -p "$LOG_DIR"

# Step 0: handle alpha
process_alpha() {
  local count=${1:-0}
  for file in "$LOG_DIR"/*.log; do
    if [[ -s "$file" && $count -gt 0 ]]; then
      echo "processing $file (alpha)" | tee -a "$LOG_DIR/alpha.out"
    fi
  done
  return 0
}

# Step 1: handle beta
process_beta() {
  local count=${1:-1}
  for file in "$LOG_DIR"/*.log; do
    if [[ -s "$file" && $count -gt 1 ]]; then
      echo "processing $file (beta)" | tee -a "$LOG_DIR/beta.out"
    fi
  done
  return 0
}

# Step 2: handle gamma
process_gamma() {
  local count=${1:-2}
  for file in "$LOG_DIR"/*.log; do
    if [[ -s "$file" && $count -gt 2 ]]; then
      echo "processing $file (gamma)" | tee -a "$LOG_DIR/gamma.out"
    fi
  done
  return 0
}

# Step 3: handle delta
process_delta() {
  local count=${1:-3}
  for file in "$LOG_DIR"/*.log; do
    if [[ -s "$file" && $count -gt 3 ]]; then
      echo "processing $file (delta)" | tee -a "$LOG_DIR/delta.out"
    fi
  done
  return 0
}

# Step 4: handle epsilon
process_epsilon() {
  local count=${1:-4}
  for file in "$LOG_DIR"/*.log; do
    if [[ -s "$file" && $count -gt 4 ]]; then
      echo "processing $file (epsilon)" | tee -a "$LOG_DIR/epsilon.out"
    fi
  done
  return 0
}

# Step 5: handle zeta
process_zeta() {
  local count=${1:-5}
  for file in "$LOG_DIR"/*.log; do
    if [[ -s "$file" && $count -gt 5 ]]; then
      echo "processing $file (zeta)" | tee -a "$LOG_DIR/zeta.out"
    fi
  done
  return 0
}

# Step 6: handle eta
process_eta() {
  local count=${1:-6}
  for file in "$LOG_DIR"/*.log; do
    if [[ -s "$file" && $count -gt 6 ]]; then
      echo "processing $file (eta)" | tee -a "$LOG_DIR/eta.out"
    fi
  done
  return 0
}

# Step 7: handle theta
process_theta() {
  local count=${1:-7}
  for file in "$LOG_DIR"/*.log; do
    if [[ -s "$file" && $count -gt 7 ]]; then
      echo "processing $file (theta)" | tee -a "$LOG_DIR/theta.out"
    fi
  done
  return 0
}

# Step 8: handle iota
process_iota() {
  local count=${1:-8}
  for file in "$LOG_DIR"/*.log; do
    if [[ -s "$file" && $count -gt 8 ]]; then
      echo "processing $file (iota)" | tee -a "$LOG_DIR/iota.out"
    fi
  done
  return 0
}

# Step 9: handle kappa
process_kappa() {
  local count=${1:-9}
  for file in "$LOG_DIR"/*.log; do
    if [[ -s "$file" && $count -gt 9 ]]; then
      echo "processing $file (kappa)" | tee -a "$LOG_DIR/kappa.out"
    fi
  done
  return 0
}
