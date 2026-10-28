# Osprey Storage in a real WinUI window [WINUI-HOST-SELF-TEST]: builds a small folder tree, opens
# Storage on it with the WinUI host and drives the window with self-test.json, saving a PNG of the
# window after each named step. Run it from a desktop session; a service session has no desktop.
#   self-test.ps1 -Exe <OspreyHost.exe> -Dll <storage.dll> -Shots <folder> [-Dumps <folder>]
# When the host fails it prints the trail the host left, and, given the folder Windows Error
# Reporting writes OspreyHost.exe's crash dumps to, the debugger's analysis of each dump.
param(
    [Parameter(Mandatory)] [string] $Exe,
    [Parameter(Mandatory)] [string] $Dll,
    [Parameter(Mandatory)] [string] $Shots,
    [string] $Dumps = '',
    [int] $Seconds = 300
)
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path $Exe).Path
$Dll = (Resolve-Path $Dll).Path
$Shots = [IO.Path]::GetFullPath($Shots)
$tree = Join-Path ([IO.Path]::GetTempPath()) 'osprey-storage-self-test'

function Write-Bytes([string] $Path, [int] $Size) {
    New-Item -ItemType Directory -Force (Split-Path $Path) | Out-Null
    [IO.File]::WriteAllBytes($Path, [byte[]]::new($Size))
}

function Hide([string] $Path) {
    $item = Get-Item -Force $Path
    $item.Attributes = $item.Attributes -bor [IO.FileAttributes]::Hidden
}

# What a person sees when the window is stuck: the whole desktop.
function Save-Desktop([string] $Path) {
    Add-Type -AssemblyName System.Drawing, System.Windows.Forms
    $screen = [Windows.Forms.Screen]::PrimaryScreen.Bounds
    $picture = [Drawing.Bitmap]::new($screen.Width, $screen.Height)
    $canvas = [Drawing.Graphics]::FromImage($picture)
    $canvas.CopyFromScreen($screen.Location, [Drawing.Point]::Empty, $screen.Size)
    $picture.Save($Path, [Drawing.Imaging.ImageFormat]::Png)
    $canvas.Dispose()
    $picture.Dispose()
}

# Everything the host left behind when it failed: its trail, the desktop, the crash and its analysis.
function Show-Failure {
    Get-Content (Join-Path $Shots 'progress.txt') -ErrorAction SilentlyContinue
    Save-Desktop (Join-Path $Shots 'failed-desktop.png')
    Get-WinEvent -FilterHashtable @{ LogName = 'Application'; ProviderName = 'Application Error' } -MaxEvents 2 -ErrorAction SilentlyContinue | Format-List TimeCreated, Message
    $cdb = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\Debuggers\x64\cdb.exe'
    if ($Dumps -and (Test-Path $cdb)) {
        Get-ChildItem $Dumps -Filter *.dmp -ErrorAction SilentlyContinue | ForEach-Object {
            "== $($_.Name)"
            & $cdb -z $_.FullName -c '.symfix; .reload; !analyze -v; q' 2>&1 | Select-String -NotMatch '^\s*$' | Select-Object -Last 150
        }
    }
}

Remove-Item $tree -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $Shots | Out-Null
Write-Bytes "$tree\Videos\clip.bin" 3MB
Write-Bytes "$tree\Code\app.bin" 512KB
Write-Bytes "$tree\Code\.git\pack.bin" 1MB
Write-Bytes "$tree\notes.txt" 10KB
Write-Bytes "$tree\.cache\big.bin" 2MB
Hide "$tree\.cache"
Hide "$tree\Code\.git"
try {
    $quoted = @('--self-test', "$PSScriptRoot\self-test.json", $Shots, $Dll, $tree) | ForEach-Object { "`"$_`"" }
    $run = Start-Process -FilePath $Exe -ArgumentList $quoted -PassThru
    $null = $run.Handle  # keeps the exit code readable after a timed wait
    if (-not $run.WaitForExit($Seconds * 1000)) {
        Show-Failure
        Stop-Process -Id $run.Id -Force
        throw "The window did not finish its self-test in $Seconds seconds"
    }
    Get-Content (Join-Path $Shots 'self-test.txt') -ErrorAction SilentlyContinue
    if ($run.ExitCode -ne 0) {
        Show-Failure
        throw "The self-test failed with exit code $($run.ExitCode)"
    }
}
finally {
    Remove-Item $tree -Recurse -Force -ErrorAction SilentlyContinue
}
