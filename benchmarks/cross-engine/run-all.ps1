<#
XB10k: times Axcenity, Godot, Unity and s&box, each run pinned to the same
CPUs, writes every run's log to logs\<engine>-<n>.log beside this script, and
prints collect.py's table at the end. Roblox and Unreal are run by hand.

    powershell -ExecutionPolicy Bypass -File run-all.ps1                 # 3 runs each
    powershell -ExecutionPolicy Bypass -File run-all.ps1 -Runs 1 -Engines godot,unity

Before it, build each engine's game once (see each engine's README):
  Axcenity  the repo's target\release\axcenity-player.exe and target\xb10k
            (generate.py axcenity target/xb10k, cargo build --release)
  Godot     <RunDir>\godot\export\XB10k.exe (release export)
  Unity     <RunDir>\unity\Builds\XB10k.exe
  s&box     the project at <RunDir>\sbox\xb10k, with Steam running

Nothing else should be building or benchmarking while it runs.
#>
param(
    [int]$Runs = 3,
    [string[]]$Engines = @("axcenity", "godot", "unity", "sbox"),
    [string]$RunDir = (Join-Path $env:USERPROFILE "Desktop\xb10k-run"),
    # CPUs 0-15: one CCD of a 16-core Ryzen, as `start /affinity FFFF`.
    [string]$Affinity = "FFFF"
)

$ErrorActionPreference = "Stop"
$here = $PSScriptRoot
$repo = (Resolve-Path (Join-Path $here "..\..")).Path
$logs = Join-Path $here "logs"
New-Item -ItemType Directory -Force $logs | Out-Null

# Runs a command pinned with `start /affinity /wait /b`, its output in $log.
function Start-Pinned([string]$exe, [string]$arguments, [string]$log, [string]$dir) {
    $line = "start `"`" /affinity $Affinity /wait /b `"$exe`" $arguments > `"$log`" 2>&1"
    Push-Location $dir
    try { cmd /c $line } finally { Pop-Location }
}

function Test-Result([string]$engine, [string]$log) {
    $text = if (Test-Path $log) { Get-Content $log -Raw } else { "" }
    $check = $text -match "XB10K_CHECK count=10000 sum=98894.50"
    $result = $text -match "XB10K_RESULT \{" -or $text -match '(?m)^\{"host"'
    Write-Host ("  {0}: checksum {1}, result {2}" -f (Split-Path $log -Leaf),
        $(if ($check) { "ok" } else { "MISSING" }), $(if ($result) { "ok" } else { "MISSING" }))
}

function Run-Axcenity([int]$n) {
    $exe = Join-Path $repo "target\release\axcenity-player.exe"
    $project = Join-Path $repo "target\xb10k"
    if (-not (Test-Path $exe) -or -not (Test-Path $project)) { throw "Build Axcenity and generate target\xb10k first" }
    $env:AXCENITY_VSYNC = "off"
    $env:AXCENITY_WINDOW_SIZE = "1920x1080"
    $env:AXCENITY_SAVE_DIR = Join-Path $project "save"
    $log = Join-Path $logs "axcenity-$n.log"
    Start-Pinned $exe "`"$project`" --bench 600" $log $repo
    Test-Result "axcenity" $log
}

function Run-Godot([int]$n) {
    $exe = Join-Path $RunDir "godot\export\XB10k.exe"
    if (-not (Test-Path $exe)) { throw "Export Godot to $exe first" }
    $log = Join-Path $logs "godot-$n.log"
    Start-Pinned $exe "--fixed-fps 60 --resolution 1920x1080" $log (Split-Path $exe)
    Test-Result "godot" $log
}

function Run-Unity([int]$n) {
    $exe = Join-Path $RunDir "unity\Builds\XB10k.exe"
    if (-not (Test-Path $exe)) { throw "Build the Unity player to $exe first" }
    $log = Join-Path $logs "unity-$n.log"
    # Unity writes its own log; -logFile puts it here.
    Start-Pinned $exe "-logFile `"$log`"" (Join-Path $logs "unity-$n.stdout.txt") (Split-Path $exe)
    Remove-Item (Join-Path $logs "unity-$n.stdout.txt") -ErrorAction SilentlyContinue
    Test-Result "unity" $log
}

# s&box: the game client in a 1920x1080 window, driven through sbox-cli.
# Vsync is read at launch (see the video settings below); the frame cap
# (fps_max, 200) can only be raised once the game runs, so each run lets the
# scene's first automatic run go by as extra warm-up, raises the cap to 1000,
# then times a second run (XB10kSystem.Restart) and logs that one with what
# s&box says limited its frame rate.
$sboxEngine = [Environment]::GetEnvironmentVariable("FACEPUNCH_ENGINE", "User")
if (-not $sboxEngine) { $sboxEngine = "C:\Program Files (x86)\Steam\steamapps\common\sbox" }
$sboxCli = Join-Path $sboxEngine "bin\managed\sbox-cli.exe"
$sboxVideo = Join-Path $sboxEngine "config\video.json"

function Invoke-Sbox([int]$processId, [string]$tool, [string]$json = "{}") {
    # One command line built by hand: Windows PowerShell 5.1 mangles quotes
    # and spaces in JSON passed to native programs.
    $info = New-Object System.Diagnostics.ProcessStartInfo $sboxCli
    $info.Arguments = "--connect $processId --call $tool `"" + ($json -replace '"', '\"') + "`""
    $info.UseShellExecute = $false
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $cli = [System.Diagnostics.Process]::Start($info)
    $out = $cli.StandardOutput.ReadToEnd() + $cli.StandardError.ReadToEnd()
    $cli.WaitForExit()
    return $out
}

function Wait-Sbox([int]$processId, [int]$since, [string]$pattern, [int]$seconds) {
    $deadline = (Get-Date).AddSeconds($seconds)
    while ((Get-Date) -lt $deadline) {
        $text = Invoke-Sbox $processId "read_console" "{`"filter`":`"XB10K`",`"since`":$since,`"limit`":50}"
        if ($text -match $pattern) { return $text }
        Start-Sleep -Seconds 2
    }
    throw "s&box: timed out waiting for $pattern"
}

function Run-Sbox([int]$n) {
    $project = Join-Path $RunDir "sbox\xb10k\xb10k.sbproj"
    if (-not (Test-Path $project)) { throw "Make the s&box project at $project first" }
    if (-not (Get-Process steam -ErrorAction SilentlyContinue)) { throw "s&box needs Steam running" }
    $log = Join-Path $logs "sbox-$n.log"
    $proc = Start-Process (Join-Path $sboxEngine "sbox.exe") -PassThru -ArgumentList @(
        "-sw", "-w", "1920", "-h", "1080", "-mcp", "+game", "`"$project`"")
    $proc.ProcessorAffinity = [IntPtr][Convert]::ToInt64($Affinity, 16)
    try {
        $first = Wait-Sbox $proc.Id 0 "XB10K_RESULT" 180
        $cursor = [int]([regex]::Match($first, "Cursor: (\d+)").Groups[1].Value)
        $script = Join-Path $logs "sbox-restart.cs"
        @'
var r = MenuUtility.RenderSettings;
r.VSync = false;
r.MaxFrameRate = 1000;
Game.ActiveScene.GetSystem<XB10kSystem>().Restart();
return new { r.VSync, r.MaxFrameRate, Screen = Screen.Size.ToString() };
'@ | Set-Content $script -Encoding utf8
        $settings = Invoke-Sbox $proc.Id "run_csharp" ("{`"path`":`"" + ($script -replace '\\', '/') + "`"}")
        Remove-Item $script
        $second = Wait-Sbox $proc.Id $cursor "XB10K_RESULT" 120
        $limiter = Invoke-Sbox $proc.Id "perf_overview"
        @("s&box run $n, settings for the timed run:", $settings.Trim(),
          "perf_overview after it: " + (($limiter -split "`n" | Select-String "LimitedBy|TargetFps") -join " "),
          $second) | Set-Content $log -Encoding utf8
        Invoke-Sbox $proc.Id "instance_console_command" '{"command":"quit"}' | Out-Null
        $proc.WaitForExit(30000) | Out-Null
    } finally {
        if (-not $proc.HasExited) { $proc.Kill() }
    }
    Test-Result "sbox" $log
}

# s&box only reads vsync when it starts, from the player's own video
# settings, so they say "off" for the runs and are put back afterwards.
$videoBackup = if (Test-Path $sboxVideo) { [IO.File]::ReadAllText($sboxVideo) } else { $null }
if ($null -ne $videoBackup -and $Engines -contains "sbox") {
    $video = $videoBackup | ConvertFrom-Json
    $video | Add-Member -Force -NotePropertyName vsync -NotePropertyValue ([pscustomobject]@{ Value = "false"; Timeout = -1; DeleteAt = 0 })
    [IO.File]::WriteAllText($sboxVideo, ($video | ConvertTo-Json -Depth 5))
}
try {
    foreach ($engine in $Engines) {
        for ($n = 1; $n -le $Runs; $n++) {
            Write-Host "$engine run $n of $Runs"
            switch ($engine) {
                "axcenity" { Run-Axcenity $n }
                "godot" { Run-Godot $n }
                "unity" { Run-Unity $n }
                "sbox" { Run-Sbox $n }
                default { throw "Unknown engine $engine" }
            }
        }
    }
} finally {
    if ($null -ne $videoBackup -and $Engines -contains "sbox") {
        [IO.File]::WriteAllText($sboxVideo, $videoBackup)
    }
}

$all = Get-ChildItem $logs -Filter "*.log" | ForEach-Object FullName
python (Join-Path $here "collect.py") @all
