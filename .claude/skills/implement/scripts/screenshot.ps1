# Runs a program, waits for its window, saves a screenshot of that window, then closes it.
# Windows only. Example (from the repository root, after `cargo build -p axcenity-player`):
#   powershell -ExecutionPolicy Bypass -File .claude/skills/implement/scripts/screenshot.ps1 `
#     -Exe target/debug/axcenity-player.exe -Arguments examples/primitives -Out <scratchpad>/shot.png
param(
    [Parameter(Mandatory)] [string] $Exe,
    [string] $Arguments = "",
    [Parameter(Mandatory)] [string] $Out,
    # Seconds to wait for the window to open and draw its first frames.
    [int] $Wait = 4
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class ShotWindow {
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    public struct RECT { public int L, T, R, B; }
}
"@

$startArgs = @{ FilePath = (Resolve-Path $Exe); WorkingDirectory = (Get-Location); PassThru = $true }
if ($Arguments) { $startArgs.ArgumentList = $Arguments }
$process = Start-Process @startArgs
try {
    Start-Sleep -Seconds $Wait
    $process.Refresh()
    if ($process.HasExited) { throw "the program exited early with code $($process.ExitCode); run it directly to see its error" }
    $handle = $process.MainWindowHandle
    if ($handle -eq [IntPtr]::Zero) { throw "the program has no window yet; try a longer -Wait" }

    [ShotWindow]::SetForegroundWindow($handle) | Out-Null
    Start-Sleep -Milliseconds 500
    $rect = New-Object ShotWindow+RECT
    [ShotWindow]::GetWindowRect($handle, [ref]$rect) | Out-Null
    $bitmap = New-Object System.Drawing.Bitmap ($rect.R - $rect.L), ($rect.B - $rect.T)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.CopyFromScreen($rect.L, $rect.T, 0, 0, $bitmap.Size)
    $bitmap.Save($Out)
    "saved $Out"
}
finally {
    if (-not $process.HasExited) { Stop-Process -Id $process.Id }
}
