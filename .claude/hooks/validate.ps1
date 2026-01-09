# Post-subagent validation script
# Shows errors but always exits 0 to avoid hook loops

if ($env:CLAUDE_PROJECT_DIR) {
    Set-Location $env:CLAUDE_PROJECT_DIR -ErrorAction SilentlyContinue
}

# Check - capture output
$checkOutput = cargo check -p iced_nodegraph 2>&1
$checkStatus = $LASTEXITCODE

# Test - capture output
$testOutput = cargo test -p iced_nodegraph 2>&1
$testStatus = $LASTEXITCODE

# Show errors if any
if ($checkStatus -ne 0) {
    Write-Output "## cargo check failed"
    $checkOutput | Select-String -Pattern "^error" | Select-Object -First 20
    Write-Output ""
}

if ($testStatus -ne 0) {
    Write-Output "## cargo test failed"
    $testOutput | Select-String -Pattern "(FAILED|panicked|error\[)" | Select-Object -First 20
    Write-Output ""
}

# Always exit 0 to avoid hook loop
exit 0
