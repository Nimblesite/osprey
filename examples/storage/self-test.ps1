# Osprey Storage in a real WinUI window [WINUI-HOST-SELF-TEST]: builds a small folder tree with
# hidden folders, opens Storage on it and runs self-test.json through the host's own runner.
#   self-test.ps1 -Exe <OspreyHost.exe> -Dll <storage.dll> -Shots <folder> [-Dumps <folder>]
param(
    [Parameter(Mandatory)] [string] $Exe,
    [Parameter(Mandatory)] [string] $Dll,
    [Parameter(Mandatory)] [string] $Shots,
    [string] $Dumps = ''
)
$ErrorActionPreference = 'Stop'
$tree = Join-Path ([IO.Path]::GetTempPath()) 'osprey-storage-self-test'

function Write-Bytes([string] $Path, [int] $Size) {
    New-Item -ItemType Directory -Force (Split-Path $Path) | Out-Null
    [IO.File]::WriteAllBytes($Path, [byte[]]::new($Size))
}

function Hide([string] $Path) {
    $item = Get-Item -Force $Path
    $item.Attributes = $item.Attributes -bor [IO.FileAttributes]::Hidden
}

Remove-Item $tree -Recurse -Force -ErrorAction SilentlyContinue
Write-Bytes "$tree\Videos\clip.bin" 3MB
Write-Bytes "$tree\Code\app.bin" 512KB
Write-Bytes "$tree\Code\.git\pack.bin" 1MB
Write-Bytes "$tree\notes.txt" 10KB
Write-Bytes "$tree\.cache\big.bin" 2MB
Hide "$tree\.cache"
Hide "$tree\Code\.git"
try {
    & (Join-Path $PSScriptRoot '..\..\hosts\winui\self-test.ps1') -Exe $Exe -Dll $Dll -Script (Join-Path $PSScriptRoot 'self-test.json') -Shots $Shots -Dumps $Dumps -Arguments @($tree)
}
finally {
    Remove-Item $tree -Recurse -Force -ErrorAction SilentlyContinue
}
