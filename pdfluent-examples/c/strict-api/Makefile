# Makefile for pdfluent-examples/c/strict-api
#
# Builds all strict-API C examples against the PDFluent native library.
# Compiles with -Wall -Wextra -Werror — must be clean on darwin-arm64 and
# linux-x86_64.
#
# Usage:
#   make                          # build all examples (plain / no G-track)
#   make CFLAGS=-DPDFLUENT_TEXT_EDITING   # build with G1-G4 stubs
#   make clean
#   make check                    # build + link smoke-check (no PDF needed)
#
# Prerequisites:
#   cargo build -p pdf-capi --release   (run from repo root first)
#
# The Makefile locates the library relative to the standard cargo output
# directory.  Override LIBDIR and INCDIR if your layout differs.

REPO_ROOT := ../../..
LIBDIR    := $(REPO_ROOT)/target/release
INCDIR    := $(REPO_ROOT)/crates/pdf-capi/include

CC        := cc
WARN      := -Wall -Wextra -Werror
CSTD      := -std=c11
INCLUDES  := -I$(INCDIR)

# Platform-specific link flags
UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Darwin)
    LDFLAGS := -L$(LIBDIR) -lpdf_capi \
               -Wl,-rpath,$(abspath $(LIBDIR)) \
               -framework Security -framework CoreFoundation
else
    LDFLAGS := -L$(LIBDIR) -lpdf_capi \
               -Wl,-rpath,$(abspath $(LIBDIR)) \
               -lpthread -ldl -lm
endif

CFLAGS ?=

ALL_CFLAGS := $(WARN) $(CSTD) $(INCLUDES) $(CFLAGS)

TARGETS := hello_world extract_text fill_form g_track

.PHONY: all clean check

all: $(TARGETS)

hello_world: hello_world.c
	$(CC) $(ALL_CFLAGS) -o $@ $< $(LDFLAGS)

extract_text: extract_text.c
	$(CC) $(ALL_CFLAGS) -o $@ $< $(LDFLAGS)

fill_form: fill_form.c
	$(CC) $(ALL_CFLAGS) -o $@ $< $(LDFLAGS)

g_track: g_track.c
	$(CC) $(ALL_CFLAGS) -o $@ $< $(LDFLAGS)

# check: compile only (syntax + warnings), no link against the library.
# Useful in CI where the Rust library may not be built yet.
check:
	$(CC) $(ALL_CFLAGS) -fsyntax-only hello_world.c
	$(CC) $(ALL_CFLAGS) -fsyntax-only extract_text.c
	$(CC) $(ALL_CFLAGS) -fsyntax-only fill_form.c
	$(CC) $(ALL_CFLAGS) -fsyntax-only g_track.c
	$(CC) $(ALL_CFLAGS) -DPDFLUENT_TEXT_EDITING -fsyntax-only extract_text.c
	$(CC) $(ALL_CFLAGS) -DPDFLUENT_TEXT_EDITING -fsyntax-only fill_form.c
	$(CC) $(ALL_CFLAGS) -DPDFLUENT_TEXT_EDITING -fsyntax-only g_track.c
	@echo "All syntax checks passed."

clean:
	rm -f $(TARGETS)
