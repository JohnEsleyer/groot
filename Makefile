GO    ?= go
CARGO ?= cargo

BIN_DIR ?= bin
TARGET  := $(BIN_DIR)/groot

.PHONY: all tools cli host monitor install clean

all: tools

# Build the full toolchain into ./bin.
tools: cli host monitor
	@echo "Groot toolchain ready in $(BIN_DIR)/"

cli:
	@mkdir -p $(BIN_DIR)
	@$(GO) build -trimpath -ldflags='-s -w' -o $(TARGET) ./cmd/groot

host:
	@$(CARGO) build --release --package groot-host
	@mkdir -p $(BIN_DIR)
	@cp target/release/groot-host $(BIN_DIR)/groot-host

monitor:
	@$(CARGO) build --release --package groot-monitor
	@mkdir -p $(BIN_DIR)
	@cp target/release/groot-monitor $(BIN_DIR)/groot-monitor

install: tools
	@./install.sh --no-build

clean:
	@rm -rf $(BIN_DIR)
	@$(CARGO) clean
