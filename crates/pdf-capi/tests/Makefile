# C API smoke test build & run
#
# Usage (from project root):
#   cargo build -p pdf-capi --release
#   make -C crates/pdf-capi/tests
#   make -C crates/pdf-capi/tests run

CC       ?= cc
CFLAGS   := -Wall -Wextra -std=c11

# Auto-detect target triple for release output directory
UNAME_M := $(shell uname -m)
UNAME_S := $(shell uname)
ifeq ($(UNAME_S), Darwin)
  ifeq ($(UNAME_M), arm64)
    TARGET_TRIPLE := aarch64-apple-darwin
  else
    TARGET_TRIPLE := x86_64-apple-darwin
  endif
  DYLIB_VAR := DYLD_LIBRARY_PATH
else
  ifeq ($(UNAME_M), aarch64)
    TARGET_TRIPLE := aarch64-unknown-linux-gnu
  else
    TARGET_TRIPLE := x86_64-unknown-linux-gnu
  endif
  DYLIB_VAR := LD_LIBRARY_PATH
endif

# Resolve TARGET_DIR to the directory that contains the built dylib.
# Prefer the triple-specific path; fall back to the generic release dir.
# This handles mixed toolchain setups where the active Rust toolchain
# targets a different architecture than the host (e.g. x86_64 toolchain
# on an arm64 Mac produces binaries in target/release/, not
# target/aarch64-apple-darwin/release/).
TARGET_DIR_TRIPLE  := ../../../target/$(TARGET_TRIPLE)/release
TARGET_DIR_GENERIC := ../../../target/release

HAS_DYLIB_TRIPLE  := $(wildcard $(TARGET_DIR_TRIPLE)/libpdf_capi.dylib \
                                $(TARGET_DIR_TRIPLE)/libpdf_capi.so)
HAS_DYLIB_GENERIC := $(wildcard $(TARGET_DIR_GENERIC)/libpdf_capi.dylib \
                                $(TARGET_DIR_GENERIC)/libpdf_capi.so)

ifneq ($(HAS_DYLIB_TRIPLE),)
  TARGET_DIR := $(TARGET_DIR_TRIPLE)
else ifneq ($(HAS_DYLIB_GENERIC),)
  TARGET_DIR := $(TARGET_DIR_GENERIC)
else
  # No dylib found yet — default to triple-specific so error message is clear.
  TARGET_DIR := $(TARGET_DIR_TRIPLE)
endif

# Compile C to match the dylib's architecture so the linker can resolve
# symbols (important when the Rust toolchain targets a different arch than
# the host compiler's default, e.g. x86_64 toolchain on arm64 macOS).
DYLIB_ARCH_RAW := $(shell file $(TARGET_DIR)/libpdf_capi.dylib \
                               $(TARGET_DIR)/libpdf_capi.so 2>/dev/null \
                  | grep -o 'x86_64\|arm64\|aarch64' | head -1)
# Normalise aarch64 → arm64 for -arch flag (clang uses arm64, not aarch64).
ifeq ($(DYLIB_ARCH_RAW),aarch64)
  CFLAGS += -arch arm64
else ifneq ($(DYLIB_ARCH_RAW),)
  CFLAGS += -arch $(DYLIB_ARCH_RAW)
endif

LDFLAGS  := -L$(TARGET_DIR)
LIBS     := -lpdf_capi
INCLUDE  := -I..

TESTS := test_smoke

.PHONY: all clean run

all: $(TESTS)

test_smoke: test_smoke.c ../pdf_capi.h
	$(CC) $(CFLAGS) $(INCLUDE) -o $@ $< $(LDFLAGS) $(LIBS)

run: all
	@cd ../../.. && $(DYLIB_VAR)=$(abspath $(TARGET_DIR)) crates/pdf-capi/tests/test_smoke

clean:
	rm -f $(TESTS)
