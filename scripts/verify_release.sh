#!/bin/sh
set -eu

cargo build --release -p rimera-runtime -p rimera-cli
target/release/rimera build tests/fixtures/basic/hello.py -o dist/hello --profile release
output="$(dist/hello)"
test "$output" = "hello"

size="$(stat -f '%z' dist/hello)"
test "$size" -le 524288

if nm dist/hello | awk '{ print $NF }' | sed 's/^_//' | grep -E '^(rv_|Py_|PyObject|setjmp$|longjmp$)' >/dev/null; then
    echo "release artifact contains a forbidden legacy symbol" >&2
    exit 1
fi

echo "release verification passed: $size bytes"
