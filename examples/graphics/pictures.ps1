# Each graphics scene on Direct3D 12, pictured after it has drawn for a while: the scene's own
# window as Windows composes it, so no console or other window in front of it gets in the way.
# Run `make _graphics-smoke` first; it builds the bridge and every scene into target/gfx.
#   pictures.ps1 -Shots <folder> [-Seconds <wait>] [-Msys <MSYS2 root>]
param(
    [Parameter(Mandatory)] [string] $Shots,
    [int] $Seconds = 20,
    [string] $Msys = $(if ($env:MSYS2_ROOT) { $env:MSYS2_ROOT } else { 'C:\msys64' })
)
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
# A scene needs the bridge beside this script and the MinGW runtime it links, which PowerShell's PATH lacks.
$env:PATH = "$PSScriptRoot;$(Join-Path $Msys 'ucrt64\bin');$env:PATH"
foreach ($scene in 'kali', 'opal', 'character') {
    & (Join-Path $root 'scripts\window-picture.ps1') -Exe (Join-Path $root "target\gfx\$scene.exe") `
        -Picture (Join-Path $Shots "$scene.png") -Seconds $Seconds
}
