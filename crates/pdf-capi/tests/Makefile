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

# Try target-specific dir first, fall back to generic release dir
TARGET_DIR := $(wildcard ../../../target/$(TARGET_TRIPLE)/release)
ifeq ($(TARGET_DIR),)
  TARGET_DIR := ../../../target/release
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
