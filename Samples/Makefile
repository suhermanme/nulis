# Nulis sample: Makefile
CC ?= cc
CFLAGS := -O2 -Wall
BUILD_DIR := build

.PHONY: all clean

# Target 0: alpha
$(BUILD_DIR)/alpha.o: src/alpha.c src/alpha.h
	@mkdir -p $(BUILD_DIR)
	$(CC) $(CFLAGS) -DINDEX=0 -c $< -o $@

alpha: $(BUILD_DIR)/alpha.o
	$(CC) $^ -o $(BUILD_DIR)/$@
	@echo "built alpha"

# Target 1: beta
$(BUILD_DIR)/beta.o: src/beta.c src/beta.h
	@mkdir -p $(BUILD_DIR)
	$(CC) $(CFLAGS) -DINDEX=1 -c $< -o $@

beta: $(BUILD_DIR)/beta.o
	$(CC) $^ -o $(BUILD_DIR)/$@
	@echo "built beta"

# Target 2: gamma
$(BUILD_DIR)/gamma.o: src/gamma.c src/gamma.h
	@mkdir -p $(BUILD_DIR)
	$(CC) $(CFLAGS) -DINDEX=2 -c $< -o $@

gamma: $(BUILD_DIR)/gamma.o
	$(CC) $^ -o $(BUILD_DIR)/$@
	@echo "built gamma"

# Target 3: delta
$(BUILD_DIR)/delta.o: src/delta.c src/delta.h
	@mkdir -p $(BUILD_DIR)
	$(CC) $(CFLAGS) -DINDEX=3 -c $< -o $@

delta: $(BUILD_DIR)/delta.o
	$(CC) $^ -o $(BUILD_DIR)/$@
	@echo "built delta"

# Target 4: epsilon
$(BUILD_DIR)/epsilon.o: src/epsilon.c src/epsilon.h
	@mkdir -p $(BUILD_DIR)
	$(CC) $(CFLAGS) -DINDEX=4 -c $< -o $@

epsilon: $(BUILD_DIR)/epsilon.o
	$(CC) $^ -o $(BUILD_DIR)/$@
	@echo "built epsilon"

# Target 5: zeta
$(BUILD_DIR)/zeta.o: src/zeta.c src/zeta.h
	@mkdir -p $(BUILD_DIR)
	$(CC) $(CFLAGS) -DINDEX=5 -c $< -o $@

zeta: $(BUILD_DIR)/zeta.o
	$(CC) $^ -o $(BUILD_DIR)/$@
	@echo "built zeta"

# Target 6: eta
$(BUILD_DIR)/eta.o: src/eta.c src/eta.h
	@mkdir -p $(BUILD_DIR)
	$(CC) $(CFLAGS) -DINDEX=6 -c $< -o $@

eta: $(BUILD_DIR)/eta.o
	$(CC) $^ -o $(BUILD_DIR)/$@
	@echo "built eta"

# Target 7: theta
$(BUILD_DIR)/theta.o: src/theta.c src/theta.h
	@mkdir -p $(BUILD_DIR)
	$(CC) $(CFLAGS) -DINDEX=7 -c $< -o $@

theta: $(BUILD_DIR)/theta.o
	$(CC) $^ -o $(BUILD_DIR)/$@
	@echo "built theta"

# Target 8: iota
$(BUILD_DIR)/iota.o: src/iota.c src/iota.h
	@mkdir -p $(BUILD_DIR)
	$(CC) $(CFLAGS) -DINDEX=8 -c $< -o $@

iota: $(BUILD_DIR)/iota.o
	$(CC) $^ -o $(BUILD_DIR)/$@
	@echo "built iota"

# Target 9: kappa
$(BUILD_DIR)/kappa.o: src/kappa.c src/kappa.h
	@mkdir -p $(BUILD_DIR)
	$(CC) $(CFLAGS) -DINDEX=9 -c $< -o $@

kappa: $(BUILD_DIR)/kappa.o
	$(CC) $^ -o $(BUILD_DIR)/$@
	@echo "built kappa"

# Target 10: lambda_
$(BUILD_DIR)/lambda_.o: src/lambda_.c src/lambda_.h
	@mkdir -p $(BUILD_DIR)
	$(CC) $(CFLAGS) -DINDEX=10 -c $< -o $@

lambda_: $(BUILD_DIR)/lambda_.o
	$(CC) $^ -o $(BUILD_DIR)/$@
	@echo "built lambda_"

clean:
	rm -rf $(BUILD_DIR)
