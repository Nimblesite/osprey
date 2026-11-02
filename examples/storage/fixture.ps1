# Osprey Storage's self-test fixture [WINUI-HOST-SELF-TEST]: a small folder tree with hidden
# folders, which hosts/winui/app.ps1 builds where the sandboxed app may read it, its own LocalState,
# and opens Storage on. Its self-test.json walks it, hides the hidden items and opens a folder.
#   fixture.ps1 -Tree <folder>
param([Parameter(Mandatory)] [string] $Tree)
$ErrorActionPreference = 'Stop'

function Write-Bytes([string] $Path, [int] $Size) {
    New-Item -ItemType Directory -Force (Split-Path $Path) | Out-Null
    [IO.File]::WriteAllBytes($Path, [byte[]]::new($Size))
}

function Hide([string] $Path) {
    $item = Get-Item -Force $Path
    $item.Attributes = $item.Attributes -bor [IO.FileAttributes]::Hidden
}

Remove-Item $Tree -Recurse -Force -ErrorAction SilentlyContinue
Write-Bytes "$Tree\Videos\clip.bin" 3MB
Write-Bytes "$Tree\Code\app.bin" 512KB
Write-Bytes "$Tree\Code\.git\pack.bin" 1MB
Write-Bytes "$Tree\notes.txt" 10KB
Write-Bytes "$Tree\.cache\big.bin" 2MB
Hide "$Tree\.cache"
Hide "$Tree\Code\.git"
