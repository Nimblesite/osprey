# Each graphics scene on Direct3D 12, pictured after it has drawn for a while: the scene's own
# window as Windows composes it, so no console or other window in front of it gets in the way.
# Run `make _graphics-smoke` first; it builds the bridge and every scene into target/gfx.
#   pictures.ps1 -Shots <folder> [-Seconds <wait>]
param(
    [Parameter(Mandatory)] [string] $Shots,
    [int] $Seconds = 20
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class SceneWindow {
    [StructLayout(LayoutKind.Sequential)] public struct Bounds { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Bounds bounds);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr window, IntPtr context, uint flags);
}
'@

function Save-Window([IntPtr] $Window, [string] $Path) {
    $bounds = New-Object SceneWindow+Bounds
    [SceneWindow]::GetWindowRect($Window, [ref] $bounds) | Out-Null
    $picture = [Drawing.Bitmap]::new($bounds.Right - $bounds.Left, $bounds.Bottom - $bounds.Top)
    $canvas = [Drawing.Graphics]::FromImage($picture)
    $context = $canvas.GetHdc()
    [SceneWindow]::PrintWindow($Window, $context, 2) | Out-Null  # PW_RENDERFULLCONTENT: the swap chain's frame too
    $canvas.ReleaseHdc($context)
    $picture.Save($Path, [Drawing.Imaging.ImageFormat]::Png)
    $canvas.Dispose()
    $picture.Dispose()
}

$root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
New-Item -ItemType Directory -Force $Shots | Out-Null
$env:PATH = "$PSScriptRoot;$env:PATH"
$env:OSPREY_GFX_FRAMES = '100000'
foreach ($scene in 'kali', 'opal', 'character') {
    $run = Start-Process (Join-Path $root "target\gfx\$scene.exe") -PassThru
    try {
        Start-Sleep -Seconds $Seconds
        $run.Refresh()
        if ($run.MainWindowHandle -eq [IntPtr]::Zero) {
            throw "$scene opened no window"
        }
        Save-Window $run.MainWindowHandle (Join-Path $Shots "$scene.png")
        "$scene pictured: $($run.MainWindowTitle)"
    }
    finally {
        Stop-Process -Id $run.Id -Force -ErrorAction SilentlyContinue
    }
}
