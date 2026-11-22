# Osprey Agent's whole UI on the WinUI host ([UI-MACHINE-SELF-TEST]), the Windows half of
# self-test.sh's doubled run: the model is a test double, every tool runs for real in a real calc
# project, the tests the agent was asked to fix are run here rather than taken on its word, and each
# picture is compared with goldens/windows/<script>/<display scale>. The agent runs unpackaged: it runs any
# command the person's account can in the folder they pick, which an AppContainer would refuse (README).
#   self-test.ps1 -Dll <agent.dll> [-Update] [-Headless]
param(
    [Parameter(Mandatory)] [string] $Dll,
    [switch] $Update,
    [switch] $Headless,
    [string] $Msys = $(if ($env:MSYS2_ROOT) { $env:MSYS2_ROOT } else { 'C:\msys64' })
)
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$repo = (Resolve-Path (Join-Path $here '..\..\..\..')).Path
$winui = Join-Path $repo 'src\toolchain\hosts\winui'
$exe = Join-Path $winui 'OspreyHost\bin\Release\net10.0-windows10.0.26100.0\win-x64\publish\OspreyHost.exe'
$records = Join-Path $repo 'target\winui-shots\agent'
# One fixed folder on every machine: its name shows in the pictures.
$work = Join-Path ([IO.Path]::GetTempPath()) 'osprey-agent-work'
$project = Join-Path $work 'calc-project'
$goldens = Join-Path $here 'goldens\windows'

function Fail([string] $Why) { throw "FAILED: $Why" }

# Run one script in the real window with the model doubled.
function Drive([string] $Name, [string] $Store, [string] $Double, [string[]] $Arguments) {
    $shots = Join-Path $records $Name
    $launch = @('--self-test', (Join-Path $here "self-test\$Name.json"), $shots, '--store', (Join-Path $work $Store))
    if ($Headless) { $launch = @('--headless') + $launch }
    if ($Double) { $launch += @('--http-double', (Join-Path $here "self-test\$Double")) }
    $run = Start-Process $exe -ArgumentList (@($launch + $Dll + $Arguments) | ForEach-Object { "`"$_`"" }) -PassThru -Wait -NoNewWindow
    if ($run.ExitCode -ne 0) { Get-Content (Join-Path $shots 'self-test.txt') -ErrorAction SilentlyContinue | Select-Object -Last 20; Fail "$Name exited with $($run.ExitCode)" }
}

Remove-Item $work, $records -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work, $records | Out-Null
try {
    # MSYS2's bash and tools by their path: the first `bash` on a Windows PATH may be WSL's.
    $env:PATH = (Join-Path $Msys 'usr\bin') + ';' + $env:PATH
    & (Join-Path $Msys 'usr\bin\bash.exe') (Join-Path $here 'drive\demo-project.sh') ($project -replace '\\', '/') | Out-Null
    if ($LASTEXITCODE -ne 0) { Fail 'the demo project was not made' }
    # The doubled model runs `python3`, as on macOS. Where that name does not run Python (Windows may have
    # only `python`, or the Store's stub), a shim earlier on the PATH the tools inherit answers to it.
    # cmd by its own path: MSYS2's usr\bin, first on the PATH, has a bash script named `cmd` that Windows
    # would offer to open with an app chooser over the window under test.
    & $env:ComSpec /d /c 'python3 -c "import sys" >nul 2>&1'
    if ($LASTEXITCODE -ne 0) {
        $shims = Join-Path $work 'bin'
        New-Item -ItemType Directory -Force $shims | Out-Null
        Set-Content (Join-Path $shims 'python3.cmd') '@python %*' -Encoding ascii
        $env:PATH = "$shims;$env:PATH"
    }
    $key = Join-Path $work 'key.env'
    Set-Content $key 'DEEPSEEKKEY=sk-double' -Encoding utf8NoBOM

    Drive 'coding' 'store-coding' 'doubles.json' @($project, $key)
    & python (Join-Path $here 'self-test\verify_calc.py') $project
    if ($LASTEXITCODE -ne 0) { Fail 'the calc project does not pass its tests' }
    if (-not (Select-String -Quiet -SimpleMatch 'wrote calc/__init__.py' (Join-Path $records 'coding\requests.jsonl'))) { Fail 'the model never heard the write''s result' }

    Drive 'code-blocks' 'store-code-blocks' 'code-blocks-doubles.json' @($project)
    Drive 'settings' 'store-settings' 'settings-doubles.json' @($project)
    & python (Join-Path $here 'self-test\verify_providers.py') (Join-Path $records 'settings\requests.jsonl')
    if ($LASTEXITCODE -ne 0) { Fail 'a provider request went to the wrong place or carried the wrong body' }
    Drive 'settings-restart' 'store-settings' '' @($project)

    $providers = Get-Content (Join-Path $work 'store-settings\providers.json') -Raw
    if ($providers -notmatch 'sk-glm-test') { Fail 'the GLM key was not saved' }
    if ($providers -match 'sk-kimi-test') { Fail 'the removed Kimi key is still saved' }
    # The step trails echo the scripts, which name the keys they check for; everything else the run kept is searched.
    $leaks = Get-ChildItem $records, (Join-Path $work 'store-settings\chats.json'), (Join-Path $work 'store-coding\chats.json') -Recurse -File -Exclude 'progress.txt', 'self-test.txt' |
        Select-String -SimpleMatch 'sk-glm-test', 'sk-kimi-test', 'sk-double'
    if ($leaks) { Fail "a key reached a saved conversation or the records: $($leaks[0].Path)" }
    foreach ($name in @('coding', 'code-blocks', 'settings', 'settings-restart')) {
        & (Join-Path $winui 'goldens.ps1') -Shots (Join-Path $records $name) -Goldens (Join-Path $goldens $name) -Update:$Update
    }
    'agent self-test passed; pictures are in ' + $records
}
finally {
    Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
}
