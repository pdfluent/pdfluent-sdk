# Convenience wrapper: build pdf-capi and run the C smoke tests.
#
# Usage (from project root or this directory):
#   cd crates/pdf-capi && make test

.PHONY: all test build clean

all: test

test: build
	$(MAKE) -C tests run

build:
	cd ../.. && cargo build -p pdf-capi --release

clean:
	$(MAKE) -C tests clean
