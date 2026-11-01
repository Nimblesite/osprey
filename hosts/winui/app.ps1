# One Osprey application on Windows, in its own sandbox [WINUI-HOST-SANDBOX]. Builds the compiler,
# the MinGW runtime, the application's DLL and the Native AOT host, lays them out as the MSIX
# package apps/windows.json describes, registers it (Developer Mode, no signing) and opens it in
# its AppContainer. With -Test it runs the application's self-test inside the sandbox instead and
# compares every picture with the goldens in <source>/goldens/windows.
#   app.ps1 -App flock                               open Flock
#   app.ps1 -App flock -Test [-Headless] [-Update]   self-test, off screen with -Headless; -Update rewrites the goldens
param(
    [Parameter(Mandatory)] [string] $App,
    [switch] $Test,
    [switch] $Headless,
    [switch] $Update,
    [string] $Msys = $(if ($env:MSYS2_ROOT) { $env:MSYS2_ROOT } else { 'C:\msys64' })
)
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$apps = Get-Content (Join-Path $repo 'apps\windows.json') -Raw | ConvertFrom-Json
$entry = $apps.apps.$App
if (-not $entry) { throw "apps/windows.json has no app called $App; it has $($apps.apps.PSObject.Properties.Name -join ', ')" }
$source = Join-Path $repo $entry.source
$layout = Join-Path $repo "target\winui\$App"
$publish = Join-Path $PSScriptRoot 'OspreyHost\bin\Release\net10.0-windows10.0.26100.0\win-x64\publish'
$restricted = @('broadFileSystemAccess')

function Invoke-Checked([string] $What, [scriptblock] $Run) {
    Write-Host "==> $What"
    & $Run
    if ($LASTEXITCODE -ne 0) { throw "$What failed with exit code $LASTEXITCODE" }
}

# The compiler, the runtime archives it links against and the application's DLL, built into the package.
function Build-Application {
    Invoke-Checked 'building the compiler' { cargo build --release -p osprey-cli --manifest-path (Join-Path $repo 'Cargo.toml') }
    $env:MSYSTEM = 'UCRT64'; $env:CHERE_INVOKING = '1'
    Invoke-Checked 'building the MinGW runtime' { & (Join-Path $Msys 'usr\bin\bash.exe') -lc "cd '$($repo -replace '\\','/')' && make _runtime CC=gcc AR=ar" }
    Remove-Item Env:MSYSTEM, Env:CHERE_INVOKING
    $env:OSPREY_CC = Join-Path $Msys 'ucrt64\bin\clang.exe'
    $env:OSPREY_AR = Join-Path $Msys 'ucrt64\bin\llvm-ar.exe'
    New-Item -ItemType Directory -Force (Join-Path $layout 'app') | Out-Null
    Invoke-Checked "building $App" { & (Join-Path $repo 'target\release\osprey.exe') build $source --target=windows-x64 --no-warnings -o (Join-Path $layout 'app\app.dll') }
}

# The Native AOT host; it links with Visual Studio's C++ tools, found through vswhere.
function Publish-Host {
    $env:PATH += ";${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer"
    Invoke-Checked 'publishing the WinUI host' { dotnet publish (Join-Path $PSScriptRoot 'OspreyHost') -c Release --nologo -v quiet }
    robocopy $publish $layout /E /NFL /NDL /NJH /NJS /NP | Out-Null
    if ($LASTEXITCODE -ge 8) { throw "copying the host into $layout failed with robocopy code $LASTEXITCODE" }
    $global:LASTEXITCODE = 0
}

function Write-Package {
    foreach ($file in $entry.files) { Copy-Item (Join-Path $source $file) (Join-Path $layout 'app') -Force }
    Set-Content (Join-Path $layout 'app\arguments.txt') -Value @($entry.arguments) -Encoding utf8
    Write-Logo (Join-Path $layout 'logo.png') $entry.name
    $capabilities = foreach ($name in $entry.capabilities) {
        if ($name -eq 'runFullTrust') { throw "$App asks for runFullTrust; an Osprey application never leaves its AppContainer" }
        if ($restricted -contains $name) { "<rescap:Capability Name=`"$name`" />" } else { "<Capability Name=`"$name`" />" }
    }
    $manifest = (Get-Content (Join-Path $PSScriptRoot 'AppxManifest.xml') -Raw).
        Replace('{Id}', $App).Replace('{Name}', $entry.name).Replace('{Description}', $entry.description).
        Replace('{Publisher}', $apps.publisher).Replace('{PublisherName}', $apps.publisherName).
        Replace('{Version}', $apps.version).Replace('{Capabilities}', ($capabilities -join ''))
    Set-Content (Join-Path $layout 'AppxManifest.xml') -Value $manifest -Encoding utf8
}

# The tile: the application's initial on a plain square, drawn here so no picture is kept in the repository.
function Write-Logo([string] $Path, [string] $Name) {
    Add-Type -AssemblyName System.Drawing
    $picture = [Drawing.Bitmap]::new(150, 150)
    $canvas = [Drawing.Graphics]::FromImage($picture)
    $canvas.Clear([Drawing.Color]::FromArgb(255, 24, 52, 44))
    $font = [Drawing.Font]::new('Segoe UI Semibold', 72)
    $format = [Drawing.StringFormat]::new(); $format.Alignment = 'Center'; $format.LineAlignment = 'Center'
    $canvas.DrawString($Name.Substring(0, 1), $font, [Drawing.Brushes]::White, [Drawing.RectangleF]::new(0, 0, 150, 150), $format)
    $picture.Save($Path, [Drawing.Imaging.ImageFormat]::Png)
    $canvas.Dispose(); $picture.Dispose(); $font.Dispose()
}

function Register-Package {
    Write-Host "==> registering Osprey.$App in its AppContainer"
    Add-AppxPackage -Register (Join-Path $layout 'AppxManifest.xml') -ForceUpdateFromAnyVersion -ForceApplicationShutdown
    (Get-AppxPackage "Osprey.$App").PackageFamilyName
}

# The self-test, run in the sandbox through the package's execution alias. The sandbox may write only
# its own LocalState, so the pictures are taken there and copied out before they are compared.
function Test-Package([string] $Family) {
    $alias = Join-Path $env:LOCALAPPDATA "Microsoft\WindowsApps\osprey-$App.exe"
    $inside = Join-Path $env:LOCALAPPDATA "Packages\$Family\LocalState\self-test"
    $shots = Join-Path $repo "target\winui-shots\$App"
    Remove-Item $inside, $shots -Recurse -Force -ErrorAction SilentlyContinue
    $arguments = @{ Exe = $alias; Dll = (Join-Path $layout 'app\app.dll'); Script = (Join-Path $layout 'app\self-test.json'); Shots = $inside }
    try { & (Join-Path $PSScriptRoot 'self-test.ps1') @arguments -Headless:$Headless }
    finally { Copy-Item $inside $shots -Recurse -Force -ErrorAction SilentlyContinue }
    & (Join-Path $PSScriptRoot 'goldens.ps1') -Shots $shots -Goldens (Join-Path $source 'goldens\windows') -Update:$Update
}

Build-Application
Publish-Host
Write-Package
$family = Register-Package
if ($Test) { Test-Package $family } else { Start-Process "shell:AppsFolder\$family!App" }
