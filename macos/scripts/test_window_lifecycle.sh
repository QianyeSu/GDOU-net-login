#!/usr/bin/env bash
set -euo pipefail

# Requires a logged-in macOS GUI session because it exercises NSStatusBarWindow.
# No credentials, SRUN requests, or reconnect services are used by this test.
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEST_DIR="$(mktemp -d "${TMPDIR:-/tmp}/gdou-window-tests.XXXXXX")"
trap 'rm -r "$TEST_DIR"' EXIT

swiftc -parse-as-library \
    -target arm64-apple-macos15.0 \
    "$ROOT_DIR/Sources/AppDelegate.swift" \
    "$ROOT_DIR/Sources/Views/MainWindowReader.swift" \
    "$ROOT_DIR/Tests/WindowLifecycleTests.swift" \
    -o "$TEST_DIR/window-lifecycle-tests"
"$TEST_DIR/window-lifecycle-tests"
