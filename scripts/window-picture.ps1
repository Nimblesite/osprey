# Starts a program, lets it draw for a while, keeps a picture of its main window and stops it.
# The picture is the window as Windows composes it (PrintWindow with PW_RENDERFULLCONTENT), so a
# swap chain's frame is pictured too and no other window in front of it gets in the way. A program
# that exits early or opens no window fails the script, so a crash is never mistaken for a picture.
#   window-picture.ps1 -Exe <program> -Picture <png> [-Seconds <wait>] [-Arguments <arguments>]
param(
    [Parameter(Mandatory)] [string] $Exe,
    [Parameter(Mandatory)] [string] $Picture,
    [int] $Seconds = 20,
    [string[]] $Arguments = @()
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class PicturedWindow {
    [StructLayout(LayoutKind.Sequential)] public struct Bounds { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Bounds bounds);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr window, IntPtr context, uint flags);
}
'@

# .NET resolves a relative path against the process's folder, not PowerShell's location.
$path = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Picture)
New-Item -ItemType Directory -Force (Split-Path -Parent $path) | Out-Null
$start = @{ FilePath = (Resolve-Path $Exe).Path; PassThru = $true }
if ($Arguments.Count -gt 0) {
    $start.ArgumentList = $Arguments
}
$run = Start-Process @start
try {
    Start-Sleep -Seconds $Seconds
    $run.Refresh()
    if ($run.HasExited) {
        throw "$Exe exited with code $($run.ExitCode) before it was pictured"
    }
    if ($run.MainWindowHandle -eq [IntPtr]::Zero) {
        throw "$Exe opened no window"
    }
    $bounds = New-Object PicturedWindow+Bounds
    [PicturedWindow]::GetWindowRect($run.MainWindowHandle, [ref] $bounds) | Out-Null
    $image = [Drawing.Bitmap]::new($bounds.Right - $bounds.Left, $bounds.Bottom - $bounds.Top)
    $canvas = [Drawing.Graphics]::FromImage($image)
    $context = $canvas.GetHdc()
    [PicturedWindow]::PrintWindow($run.MainWindowHandle, $context, 2) | Out-Null  # PW_RENDERFULLCONTENT
    $canvas.ReleaseHdc($context)
    $image.Save($path, [Drawing.Imaging.ImageFormat]::Png)
    $canvas.Dispose()
    $image.Dispose()
    "$(Split-Path -Leaf $Exe) pictured: $($run.MainWindowTitle)"
}
finally {
    Stop-Process -Id $run.Id -Force -ErrorAction SilentlyContinue
}
