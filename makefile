# -----------------------------------------
#          Rimera Lite Build               
# -----------------------------------------
OUTPUT_DIR = "dist/rimera-release"

# -----------------------------------------
# Available Commands
# -----------------------------------------
# List of commands available for make.
.PHONY: all build run clean dir


# -----------------------------------------
# Default command
# -----------------------------------------
all: build


# -----------------------------------------
# Build for release
# -----------------------------------------
# Don't build for debug here !!!
build:
	@mkdir -p $(OUTPUT_DIR)
	cargo build --release
	mv target/release/rimera-lite $(OUTPUT_DIR)/rimera-lite
	
# -----------------------------------------
# Run the program
# -----------------------------------------
#
# Cargo builds the project if necessary
# and then runs it.
run:
	cargo run


# -----------------------------------------
# Clean build files
# -----------------------------------------
clean:
	cargo clean

dir:
	@echo "Using Output Dir: ./$(OUTPUT_DIR)"