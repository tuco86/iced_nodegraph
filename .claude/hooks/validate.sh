#!/bin/bash
# Post-subagent validation script
# Only outputs on errors to avoid filling context

cd "$CLAUDE_PROJECT_DIR" || exit 0

# Format (silent, just fix)
cargo fmt --all 2>/dev/null

# Check - capture output
check_output=$(cargo check -p iced_nodegraph 2>&1)
check_status=$?

# Test - capture output
test_output=$(cargo test -p iced_nodegraph 2>&1)
test_status=$?

# Only output if there were errors
if [ $check_status -ne 0 ]; then
    echo "## cargo check failed"
    echo "$check_output" | grep -E "^error" | head -20
    echo ""
fi

if [ $test_status -ne 0 ]; then
    echo "## cargo test failed"
    echo "$test_output" | grep -E "(FAILED|panicked|error\[)" | head -20
    echo "$test_output" | grep -E "^test .* FAILED" | head -10
    echo ""
fi

# Exit 2 if any errors (shows to Claude)
if [ $check_status -ne 0 ] || [ $test_status -ne 0 ]; then
    exit 2
fi

exit 0
