# Examples Makefile — builds C examples against the release dylib.
#
# Usage (from project root):
#   cargo build -p pdf-capi --release
#   make -C crates/pdf-capi/examples
#
# Run each example:
#   make -C crates/pdf-capi/examples run-license_activate

CC      ?= cc
CFLAGS  := -Wall -Wextra -Werror -std=c11 -I../include

# Auto-detect target triple (matches tests/Makefile logic).
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
  TARGET_DIR := $(TARGET_DIR_TRIPLE)
endif

# Match the dylib architecture so the linker resolves symbols correctly.
DYLIB_ARCH_RAW := $(shell file $(TARGET_DIR)/libpdf_capi.dylib \
                               $(TARGET_DIR)/libpdf_capi.so 2>/dev/null \
                  | grep -o 'x86_64\|arm64\|aarch64' | head -1)
ifeq ($(DYLIB_ARCH_RAW),aarch64)
  CFLAGS += -arch arm64
else ifneq ($(DYLIB_ARCH_RAW),)
  CFLAGS += -arch $(DYLIB_ARCH_RAW)
endif

LDFLAGS := -L$(TARGET_DIR)
LIBS    := -lpdf_capi

EXAMPLES := license_activate

.PHONY: all clean $(addprefix run-,$(EXAMPLES))

all: $(EXAMPLES)

license_activate: license_activate.c ../include/pdfluent.h
	$(CC) $(CFLAGS) -o $@ $< $(LDFLAGS) $(LIBS)

$(addprefix run-,$(EXAMPLES)): run-%: %
	@cd ../../.. && $(DYLIB_VAR)=$(abspath $(TARGET_DIR)) crates/pdf-capi/examples/$*

clean:
	rm -f $(EXAMPLES)
