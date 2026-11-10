# Checks this Windows computer without GitHub: what CI runs, plus the game
# test runner on an example and the dedicated server. macOS and Linux:
# scripts/check-platform.sh. See docs/development.md ("Checking another
# platform").
#
#   powershell -ExecutionPolicy Bypass -File scripts\check-platform.ps1
#
# Every step runs even if one fails. The summary is printed at the end and
# saved to target\platform-check-windows.txt; the exit code is 1 if any failed.

Set-Location (Join-Path $PSScriptRoot "..")
$summary = "target\platform-check-windows.txt"
$results = @()
$failed = 0

function Step($name, [scriptblock]$run) {
    Write-Host ""
    Write-Host "==> $name"
    $global:LASTEXITCODE = 0
    & $run
    if ($LASTEXITCODE -eq 0) {
        $script:results += "PASS  $name"
    } else {
        $script:results += "FAIL  $name"
        $script:failed = 1
    }
}

Step "Build" { cargo build --workspace --all-targets --locked }
Step "The player has no editor code" {
    $tree = cargo tree -p axcenity-player -e normal --prefix none --locked
    $editor = $tree | Select-String -Pattern '^(axcenity-editor|axcenity-blocks|axcenity-plugins|axcenity-cook|intel_tex_2|eframe|egui|subsecond) '
    if ($editor) { $editor; $global:LASTEXITCODE = 1 }
}
Step "Clippy" { cargo clippy --workspace --all-targets --locked -- -D warnings }
Step "Every test" {
    cargo nextest --version *> $null
    if ($LASTEXITCODE -eq 0) {
        cargo nextest run --workspace --locked
    } else {
        Write-Host "(cargo-nextest isn't installed: using cargo test)"
        cargo test --workspace --locked --no-fail-fast
    }
}
Step "Doc tests" { cargo test --workspace --doc --locked }
Step "axcenity-test examples/hello-character" {
    cargo run -p axcenity-testing --locked -- examples/hello-character
}
Step "axcenity-server headless" { cargo test -p axcenity-server --test headless --locked }

New-Item -ItemType Directory -Force target | Out-Null
$commit = git rev-parse --short HEAD
$lines = @("Platform check on Windows $([Environment]::OSVersion.Version), commit $commit, $(Get-Date)") + $results
$lines | Tee-Object -FilePath $summary
Write-Host "Saved to $summary"
exit $failed
