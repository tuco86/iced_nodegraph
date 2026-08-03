# Stop-hook validation: fmt check, native check, wasm check, tests.
# Shows errors but always exits 0 to avoid hook loops.
# Keep in sync with the bash twin, .claude/hooks/validate.sh.

if ($env:CLAUDE_PROJECT_DIR) {
    Set-Location $env:CLAUDE_PROJECT_DIR -ErrorAction SilentlyContinue
}

# Format check - capture output
$fmtOutput = cargo fmt --all --check 2>&1
$fmtStatus = $LASTEXITCODE

# Check native - capture output
$checkOutput = cargo check -p iced_nodegraph 2>&1
$checkStatus = $LASTEXITCODE

# Check WASM - capture output
$wasmOutput = cargo check -p iced_nodegraph --target wasm32-unknown-unknown 2>&1
$wasmStatus = $LASTEXITCODE

# Test - capture output
$testOutput = cargo test -p iced_nodegraph 2>&1
$testStatus = $LASTEXITCODE


# Show errors if any
if ($fmtStatus -ne 0) {
    Write-Output "## cargo fmt --check failed"
    Write-Output "run 'cargo fmt --all' to fix formatting"
    $fmtOutput | Select-String -Pattern "^Diff in" | Select-Object -First 20
    Write-Output ""
}
# Show errors if any
if ($checkStatus -ne 0) {
    Write-Output "## cargo check (native) failed"
    $checkOutput | Select-String -Pattern "^error" | Select-Object -First 20
    Write-Output ""
}

if ($wasmStatus -ne 0) {
    Write-Output "## cargo check (wasm) failed"
    $wasmOutput | Select-String -Pattern "^error" | Select-Object -First 20
    Write-Output ""
}

if ($testStatus -ne 0) {
    Write-Output "## cargo test failed"
    $testOutput | Select-String -Pattern "(FAILED|panicked|error\[)" | Select-Object -First 20
    $testOutput | Select-String -Pattern "^test .* FAILED" | Select-Object -First 10
    Write-Output ""
}

# Always exit 0 to avoid Stop-hook loop. exit 2 would block stopping and
# re-invoke Claude on every failure, causing an endless loop. Errors above
# are printed for visibility but must not gate the Stop event.
exit 0
