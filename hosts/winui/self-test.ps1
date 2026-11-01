# An Osprey Native application in a real WinUI window [WINUI-HOST-SELF-TEST]: opens the host on
# the application's DLL, drives the window with a self-test script and keeps a PNG of the window
# after each named step. Run it from a desktop session; a Windows service session has no desktop.
#   self-test.ps1 -Exe <OspreyHost.exe> -Dll <app.dll> -Script <steps.json> -Shots <folder>
#                 [-Dumps <folder>] [-Arguments <launch arguments>] [-Seconds <limit>] [-Headless]
# -Exe may be a packaged application's execution alias, which runs it in its AppContainer.
# -Headless draws the window off screen without activating it; by default the window is on screen.
# When the host fails it prints the trail the host left and keeps a picture of the desktop, and,
# given the folder Windows Error Reporting writes OspreyHost.exe's crash dumps to, the debugger's
# analysis of each dump.
param(
    [Parameter(Mandatory)] [string] $Exe,
    [Parameter(Mandatory)] [string] $Dll,
    [Parameter(Mandatory)] [string] $Script,
    [Parameter(Mandatory)] [string] $Shots,
    [string] $Dumps = '',
    [string[]] $Arguments = @(),
    [int] $Seconds = 300,
    [switch] $Headless
)
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path $Exe).Path
$Dll = (Resolve-Path $Dll).Path
$Script = (Resolve-Path $Script).Path
$Shots = [IO.Path]::GetFullPath($Shots)
New-Item -ItemType Directory -Force $Shots | Out-Null

# What a person sees: the whole desktop.
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

# Windows keeps a new window behind the one a person is typing in, and WinUI draws nothing in a
# window that is never shown, so the harness brings the host's window to the front the way test
# tools do: a mouse move of no distance lifts the foreground lock without a keyboard cue, which
# would draw focus rings in the pictures, then the window is restored and raised.
function Show-Window([Diagnostics.Process] $Process) {
    Add-Type -Namespace SelfTest -Name Foreground -MemberDefinition @'
[DllImport("user32.dll")] public static extern void mouse_event(uint flags, int x, int y, uint data, UIntPtr extra);
[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
[DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr window, int command);
'@
    for ($tries = 0; $tries -lt 300 -and $Process.MainWindowHandle -eq 0 -and -not $Process.HasExited; $tries++) {
        Start-Sleep -Milliseconds 100
        $Process.Refresh()
    }
    if ($Process.MainWindowHandle -eq 0) { return }
    [SelfTest.Foreground]::mouse_event(0x0001, 0, 0, 0, [UIntPtr]::Zero)
    $null = [SelfTest.Foreground]::ShowWindow($Process.MainWindowHandle, 9)
    $null = [SelfTest.Foreground]::SetForegroundWindow($Process.MainWindowHandle)
}

# Each argument quoted as Windows reads a command line: backslashes before the closing quote are
# doubled, or a folder such as C:\ would arrive as C:" and swallow the arguments after it.
function Format-Argument([string] $Text) {
    '"' + ($Text -replace '(\\+)$', '$1$1') + '"'
}
$quoted = @(if ($Headless) { '--headless' }) + @('--self-test', $Script, $Shots, $Dll) + $Arguments | ForEach-Object { Format-Argument $_ }
$run = Start-Process -FilePath $Exe -ArgumentList $quoted -PassThru
$null = $run.Handle  # keeps the exit code readable after a timed wait
if (-not $Headless) { Show-Window $run }
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
