# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# The toolchain half of the self-hosted release runners (#636), Windows.
# Called by .github/actions/setup-selfhosted with the tools one job needs;
# the Linux/macOS half is selfhosted_setup.sh.
#
#   powershell -File scripts/release/selfhosted_setup.ps1 -Tools rust,python `
#       -RustVersion 1.94.0 -NodeVersion 20 -PythonVersion 3.12 `
#       -MaturinVersion 1.15.0 -JavaVersion 17 -DotnetVersion 8 `
#       -NapiVersion 2.18.0 -MavenVersion 3.9.9
#
# Every tool is pinned to the version the release workflows already use
# (node excepted: the workflows use 24 and pass it explicitly; the bare
# default below is the #636 pin for standalone runs). Tools is a subset of
# rust node python maturin java dotnet wasm napi zig docker, installed in
# dependency order regardless of the order named. -NodeVersion takes a major
# (20) or an exact version (20.18.1); -PythonVersion takes a minor only.
#
# IDEMPOTENT: a tool already present at the wanted version is kept and only
# its version is logged. Every tool logs its version when it is done, so a
# run shows what the job built with.
#
# The MSVC build tools are checked FIRST, before anything is installed: the
# self-hosted Windows image provides them (see scripts/cos/gcp_runner.sh),
# and without them no Rust or C build on this runner can link. docker is
# accepted and skipped: no release job needs containers on Windows.
#
# Python comes from python.org's Windows installer, pinned per minor in
# $PythonPins below (patch + sha256 of the amd64 exe, verified after
# download, refused on mismatch), installed machine-wide to C:\Python<MM>.
# The public windows-2022 image carries no package manager, so every
# installer is downloaded directly: this script must work on a bare VM.
#
# Machine settings (long paths, git longpaths, and, on the ephemeral VMs
# only, Defender exclusions) are applied once per VM by Set-MachineSettings,
# idempotently, and the bash the workflows' shell: bash steps need is checked
# by Test-BashReady; both run before any tool is installed and refuse clearly
# instead of limping on. Defender exclusions are opt-in through
# PDFLUENT_SELFHOSTED_DEFENDER_EXCLUSIONS=1, which only the ephemeral VM
# images set: without it the setup never touches Defender and a leg runs
# with real-time protection on, warned once, only slower.
#
# Zip-based tools (node, temurin, maven) live under C:\pdfluent-tools,
# machine-wide, so a SYSTEM job and an operator's shell see the same install.
#
# New bins are appended to GITHUB_PATH (and JAVA_HOME/DOTNET_ROOT/VS env to
# GITHUB_ENV) so the steps after the setup see them; outside a run the
# export lines are printed instead.
param(
    [string]$Tools = "",
    [string]$RustVersion = "1.94.0",
    [string]$RustTargets = "",
    [string]$NodeVersion = "20",
    [string]$PythonVersion = "3.12",
    [string]$MaturinVersion = "1.15.0",
    [string]$JavaVersion = "17",
    [string]$DotnetVersion = "8",
    [string]$NapiVersion = "2.18.0",
    [string]$MavenVersion = "3.9.9"
)

$ErrorActionPreference = "Stop"

function Log([string]$Message) {
    Write-Host "selfhosted_setup: $Message"
}

function Die([string]$Message) {
    Write-Host "selfhosted_setup: REFUSED: $Message"
    exit 2
}

function Want([string]$Name) {
    return (",$Tools," -like "*,$Name,*")
}

function Add-ToPath([string]$Dir) {
    if ($env:GITHUB_PATH) {
        Add-Content -Path $env:GITHUB_PATH -Value $Dir
    } else {
        Log "export PATH=""$Dir;`$PATH"""
    }
    $env:PATH = "$Dir;$env:PATH"
}

function Set-StepEnv([string]$Name, [string]$Value) {
    if ($env:GITHUB_ENV) {
        Add-Content -Path $env:GITHUB_ENV -Value "$Name=$Value"
    } else {
        Log "export $Name=""$Value"""
    }
    [Environment]::SetEnvironmentVariable($Name, $Value, "Process")
}

function Fetch([string]$Url, [string]$Dest) {
    $curl = Get-Command curl.exe -ErrorAction SilentlyContinue
    if (-not $curl) { Die "curl.exe is missing: the Windows image must carry it" }
    & curl.exe -fsSL --retry 3 --retry-delay 5 -o $Dest $Url
    if ($LASTEXITCODE -ne 0) { Die "download failed: $Url" }
    if (-not (Test-Path $Dest)) { Die "download failed: $Url" }
}

# Machine-wide root for the zip-based tools (node, temurin, maven).
$ToolsRoot = "C:\pdfluent-tools"

# Python per minor: the latest patch with an official python.org amd64
# installer, plus the sha256 of that exe. python.org's release pages print
# md5, not sha256; both exes below were downloaded over TLS and matched the
# page's md5 (3.11.9 e8dcd502e34932eebcaf1be056d5cbcd, 3.12.10
# 5eddb0b6f12c852725de071ae681dde4) before their sha256 was pinned here.
# A new row must repeat that: match the page first, then pin the sha256.
$PythonPins = @{
    "3.11" = @{ Patch = "3.11.9"; Sha256 = "5ee42c4eee1e6b4464bb23722f90b45303f79442df63083f05322f1785f5fdde" }
    "3.12" = @{ Patch = "3.12.10"; Sha256 = "67b5635e80ea51072b87941312d00ec8927c4db9ba18938f7ad2d27b328b95fb" }
}

function Test-IsElevated {
    $id = [Security.Principal.WindowsIdentity]::GetCurrent()
    $p = New-Object Security.Principal.WindowsPrincipal($id)
    return $p.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Set-MachineSettings {
    # Once per VM, idempotent. Long paths (node_modules and cargo target
    # dirs outgrow MAX_PATH) and git longpaths are REQUIRED: a leg cannot
    # build without them, so a shell that cannot set them refuses. Defender
    # exclusions for the runner root, the cargo/rustup homes and the tools
    # dir are OPT-IN instead: only when
    # PDFLUENT_SELFHOSTED_DEFENDER_EXCLUSIONS is 1 (set on the ephemeral VM
    # images, never on the persistent desktop runner) are they added,
    # idempotently, and required. Without the opt-in the setup never adds
    # or requires an exclusion: when real-time protection is on it logs one
    # WARNING and continues, and the leg runs with Defender active, only
    # slower. Runs as SYSTEM on the self-hosted runners; refuses clearly
    # when not elevated and something required must still change, and logs
    # each setting either way.
    $fsKey = "HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem"
    $lp = Get-ItemProperty -Path $fsKey -Name LongPathsEnabled -ErrorAction SilentlyContinue
    $needLp = ((-not $lp) -or ($lp.LongPathsEnabled -ne 1))
    $git = Get-Command git -ErrorAction SilentlyContinue
    $needGit = $false
    if ($git) {
        $needGit = ((& git config --system core.longpaths 2>$null) -ne "true")
    }
    $mp = Get-Command Get-MpPreference -ErrorAction SilentlyContinue
    $runnerRoot = "C:\actions-runner"
    if ($env:GITHUB_WORKSPACE) {
        $parts = $env:GITHUB_WORKSPACE -split '[\\/]'
        if ($parts.Count -ge 2 -and $parts[0] -and $parts[1]) {
            $runnerRoot = "$($parts[0])\$($parts[1])"
        }
    }
    $defenderOptIn = ($env:PDFLUENT_SELFHOSTED_DEFENDER_EXCLUSIONS -eq "1")
    $wantExcl = @($runnerRoot, (Join-Path $env:USERPROFILE ".cargo"), (Join-Path $env:USERPROFILE ".rustup"), $ToolsRoot)
    $missingExcl = @()
    if ($defenderOptIn -and $mp) {
        $haveExcl = @((& Get-MpPreference).ExclusionPath)
        foreach ($e in $wantExcl) {
            if ($haveExcl -notcontains $e) { $missingExcl += $e }
        }
    }
    if (-not $defenderOptIn -and $mp) {
        $rtmOff = (& Get-MpPreference).DisableRealtimeMonitoring
        if (-not $rtmOff) {
            Log "WARNING: Defender real-time protection is on for $($wantExcl -join '; '); builds may be slower; set PDFLUENT_SELFHOSTED_DEFENDER_EXCLUSIONS=1 on ephemeral VMs only"
        }
    }
    if (-not $needLp -and -not $needGit -and $missingExcl.Count -eq 0) {
        Log "machine settings: already set (LongPathsEnabled=1, git core.longpaths)"
        if (-not $git) { Log "machine settings: git not present, core.longpaths skipped" }
        if (-not $mp) { Log "machine settings: Get-MpPreference unavailable, Defender exclusions skipped" }
        elseif (-not $defenderOptIn) { Log "machine settings: Defender exclusions not requested (PDFLUENT_SELFHOSTED_DEFENDER_EXCLUSIONS is not 1)" }
        else { Log "machine settings: Defender exclusions already present" }
        return
    }
    if (-not (Test-IsElevated)) {
        $missing = @()
        if ($needLp) { $missing += "LongPathsEnabled=1" }
        if ($needGit) { $missing += "git core.longpaths=true" }
        foreach ($e in $missingExcl) { $missing += "Defender exclusion $e" }
        Die "machine settings need an elevated shell (SYSTEM/admin); still missing: $($missing -join '; ')"
    }
    if ($needLp) {
        Set-ItemProperty -Path $fsKey -Name LongPathsEnabled -Value 1 -Type DWord
        Log "machine settings: LongPathsEnabled=1"
    } else {
        Log "machine settings: LongPathsEnabled already 1"
    }
    if (-not $git) {
        Log "machine settings: git not present, core.longpaths skipped"
    } elseif ($needGit) {
        & git config --system core.longpaths true
        if ($LASTEXITCODE -ne 0) { Die "git config --system core.longpaths true failed" }
        Log "machine settings: git core.longpaths=true"
    } else {
        Log "machine settings: git core.longpaths already true"
    }
    if (-not $mp) {
        Log "machine settings: Get-MpPreference unavailable, Defender exclusions skipped"
    } elseif (-not $defenderOptIn) {
        Log "machine settings: Defender exclusions not requested (PDFLUENT_SELFHOSTED_DEFENDER_EXCLUSIONS is not 1)"
    } elseif ($missingExcl.Count -gt 0) {
        Add-MpPreference -ExclusionPath $missingExcl
        foreach ($e in $missingExcl) {
            Log "machine settings: Defender exclusion $e"
        }
    } else {
        Log "machine settings: Defender exclusions already present"
    }
}

function Test-BashReady {
    # The workflows run their steps with shell: bash, which on these runners
    # is Git for Windows' bash. Refuse when it is missing or when PATH
    # resolves bash.exe to the WSL launcher instead.
    $b = Get-Command bash.exe -ErrorAction SilentlyContinue
    if (-not $b) {
        Die "no bash.exe on PATH: install Git for Windows (MinGit has no bash); the workflows run their steps with shell: bash"
    }
    if ($b.Source -ieq "C:\Windows\System32\bash.exe") {
        Die "bash.exe resolves to C:\Windows\System32\bash.exe, the WSL launcher: install Git for Windows (MinGit has no bash); the WSL bash cannot run the workflows' steps"
    }
    $bv = (& bash --version | Select-Object -First 1)
    Log "bash $bv at $($b.Source)"
}

# --- MSVC first: without it nothing here can link ---------------------------
$VsWhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$VsDevCmd = $null
if (Test-Path $VsWhere) {
    $VsDevCmd = & $VsWhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -find 'Common7\Tools\VsDevCmd.bat' | Select-Object -First 1
}
if (-not $VsDevCmd) {
    Die "no MSVC build tools found: install Visual Studio 2022 Build Tools with the C++ workload (the self-hosted Windows image provides them; see scripts/cos/gcp_runner.sh)"
}
Log "MSVC developer prompt: $VsDevCmd"
# The prompt's environment (PATH, INCLUDE, LIB, ...) is what lets cargo link:
# capture it once and carry it into the steps after this one.
$VsEnv = & cmd /c "`"$VsDevCmd`" -arch=x64 >NUL && set"
foreach ($Line in $VsEnv) {
    $Eq = $Line.IndexOf("=")
    if ($Eq -le 0) { continue }
    $Name = $Line.Substring(0, $Eq)
    $Value = $Line.Substring($Eq + 1)
    if ($Name -match '^(PATH|INCLUDE|LIB|LIBPATH|VC.*|VSCMD.*|WindowsSDK.*|UCRT.*)$') {
        Set-StepEnv $Name $Value
    }
}
# The steps run in Git for Windows' bash, whose PATH puts C:\Git\usr\bin
# (coreutils' `link`, the hard-link tool) ahead of MSVC's linker; with
# VCINSTALLDIR set, rustc trusts PATH and called the wrong link.exe (smoke run
# 37335333934). Name MSVC's linker for cargo outright.
$MsvcLink = Join-Path $env:VCToolsInstallDir "bin\HostX64\x64\link.exe"
if (-not (Test-Path $MsvcLink)) { Die "MSVC link.exe is not at $MsvcLink (VCToolsInstallDir from the developer prompt)" }
Set-StepEnv "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER" $MsvcLink
Log "cargo linker: $MsvcLink"
# The C smoke looks for clang: the Build Tools' Clang component lives under the
# install's VC\Tools\Llvm, which the developer prompt does not put on PATH.
$VsRoot = & $VsWhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath | Select-Object -First 1
if ($VsRoot) {
    $LlvmBin = Join-Path $VsRoot "VC\Tools\Llvm\x64\bin"
    if (Test-Path (Join-Path $LlvmBin "clang.exe")) {
        Add-ToPath $LlvmBin
        Log "clang: $LlvmBin"
    } else {
        Log "clang: not in the Build Tools ($LlvmBin); the C smoke falls back to its own lookup"
    }
}

# --- argument check before anything is installed -----------------------------
$Known = ",rust,node,python,maturin,java,dotnet,wasm,napi,zig,docker,"
if (-not $Tools) { Die "-Tools is required (a subset of rust node python maturin java dotnet wasm napi zig docker)" }
foreach ($Name in $Tools.Split(",")) {
    if ($Known -notlike "*,$Name,*") {
        Die "unknown tool $Name (want a subset of rust node python maturin java dotnet wasm napi zig docker)"
    }
}

# Machine settings and the bash check run on every job, before any tool:
# both are idempotent, and a job that would build on long paths off or
# without bash must hear it now rather than fail mid-build.
Set-MachineSettings
Test-BashReady

function Setup-Rust {
    $rustup = Get-Command rustup -ErrorAction SilentlyContinue
    if (-not $rustup) {
        Log "installing rustup"
        $tmp = Join-Path $env:TEMP "rustup-init.exe"
        Fetch "https://win.rustup.rs/x86_64" $tmp
        & $tmp -y --profile minimal --default-toolchain none
        if ($LASTEXITCODE -ne 0) { Die "rustup-init failed" }
        Remove-Item $tmp -Force
    }
    Add-ToPath (Join-Path $env:USERPROFILE ".cargo\bin")
    $listed = & rustup toolchain list 2>$null
    if (-not ($listed -match "^$RustVersion")) {
        Log "installing rust $RustVersion"
        & rustup toolchain install $RustVersion --profile minimal -c rustfmt -c clippy
        if ($LASTEXITCODE -ne 0) { Die "rustup toolchain install $RustVersion failed" }
    }
    & rustup default $RustVersion | Out-Null
    foreach ($Target in $RustTargets.Split(@(" ", ","), [StringSplitOptions]::RemoveEmptyEntries)) {
        $installed = & rustup target list --toolchain $RustVersion --installed 2>$null
        if (-not ($installed -match "^$Target")) {
            Log "adding rust target $Target"
            & rustup target add --toolchain $RustVersion $Target
            if ($LASTEXITCODE -ne 0) { Die "rustup target add $Target failed" }
        }
    }
    $rver = (& rustc --version).Split(" ")[1]
    $cver = (& cargo --version).Split(" ")[1]
    Log "rustc $rver / cargo $cver"
}

function Setup-Node {
    $want = $NodeVersion.Trim()
    $exact = $null
    $major = $null
    if ($want -match '^\d+$') {
        $major = $want
    } elseif ($want -match '^\d+\.\d+\.\d+$') {
        $exact = $want
        $major = $want.Split(".")[0]
    } else {
        Die "node: want a major (20) or an exact version (20.18.1), got '$NodeVersion'"
    }
    $node = Get-Command node -ErrorAction SilentlyContinue
    if ($node) {
        $haveFull = (& node --version).TrimStart("v")
        if (($exact -and $haveFull -eq $exact) -or ((-not $exact) -and $haveFull.Split(".")[0] -eq $major)) {
            $npver = (& npm --version)
            Log "node $(& node --version) / npm $npver (already present)"
            return
        }
        Log "node is $haveFull, want $want; installing beside it"
    }
    if ($exact) {
        $ver = "v$exact"
        Log "installing node $ver (win-x64, exact pin)"
    } else {
        Log "resolving node $major"
        $index = Invoke-RestMethod -Uri "https://nodejs.org/dist/index.json"
        $ver = $null
        foreach ($row in $index) {
            if ($row.version.Split(".")[0] -eq "v$major") { $ver = $row.version; break }
        }
        if (-not $ver) { Die "node: no release found for major $major" }
        Log "node major $major resolves to $ver"
        Log "installing node $ver (win-x64)"
    }
    $tmp = Join-Path $env:TEMP "node.zip"
    Fetch "https://nodejs.org/dist/$ver/node-$ver-win-x64.zip" $tmp
    $dest = Join-Path $ToolsRoot "node-$ver-win-x64"
    if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }
    New-Item -ItemType Directory -Path $ToolsRoot -Force | Out-Null
    Expand-Archive -Path $tmp -DestinationPath $ToolsRoot -Force
    Remove-Item $tmp -Force
    Add-ToPath $dest
    Log "node $(& node --version) / npm $(& npm --version)"
}

# The workflows' bash steps call `python3`, which the python.org
# installer does not ship (build 3: `python3: command not found` on
# win-x64): a copy of python.exe beside it, in the dir the setup
# already puts on PATH, so Git Bash resolves both names.
function Set-Python3Shim {
    param([string]$TargetDir)
    $shim = Join-Path $TargetDir "python3.exe"
    Copy-Item -Path (Join-Path $TargetDir "python.exe") -Destination $shim -Force
    Log "python3 shim: $shim"
}

function Setup-Python {
    $minor = $PythonVersion.Trim()
    if (-not $PythonPins.ContainsKey($minor)) {
        Die "python: no pinned patch for '$PythonVersion' (want one of: $($PythonPins.Keys -join ', '))"
    }
    $pin = $PythonPins[$minor]
    $patch = $pin.Patch
    $targetDir = "C:\Python" + ($minor -replace '\.', '')
    $exe = Join-Path $targetDir "python.exe"
    if (Test-Path $exe) {
        $v = (& $exe --version 2>&1).Split(" ")[1].Split(".")
        if ("$($v[0]).$($v[1])" -eq $minor) {
            Add-ToPath $targetDir
            Add-ToPath (Join-Path $targetDir "Scripts")
            Set-Python3Shim $targetDir
            & $exe -m pip --version 2>$null | Out-Null
            if ($LASTEXITCODE -ne 0) {
                Log "installing pip"
                & $exe -m ensurepip --upgrade
            }
            $pver = (& $exe --version 2>&1).Split(" ")[1]
            $mver = (& $exe -m pip --version 2>$null).Split(" ")[1]
            Log "python $pver / pip $mver (already present)"
            return
        }
        Log "$exe reports $($v -join '.'), want $minor; reinstalling"
    }
    Log "installing python $patch (python.org amd64, machine-wide)"
    $tmp = Join-Path $env:TEMP "python-$patch-amd64.exe"
    Fetch "https://www.python.org/ftp/python/$patch/python-$patch-amd64.exe" $tmp
    $hash = (Get-FileHash -Path $tmp -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($hash -ne $pin.Sha256) {
        Remove-Item $tmp -Force
        Die "python $patch installer sha256 mismatch: got $hash, want $($pin.Sha256)"
    }
    # The installer is a GUI-subsystem exe: the call operator returns at once
    # and leaves $LASTEXITCODE unset, so wait for it explicitly (smoke run
    # 37310783497 tried to delete the still-running installer).
    $proc = Start-Process -FilePath $tmp -ArgumentList "/quiet", "InstallAllUsers=1", "PrependPath=0", "Include_test=0", "TargetDir=$targetDir" -Wait -PassThru
    if ($proc.ExitCode -notin 0, 3010) { Die "python $patch installer failed (exit $($proc.ExitCode))" }
    Remove-Item $tmp -Force
    if (-not (Test-Path $exe)) { Die "python $patch installed but $exe is missing" }
    Add-ToPath $targetDir
    Add-ToPath (Join-Path $targetDir "Scripts")
    Set-Python3Shim $targetDir
    & $exe -m pip --version 2>$null | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Log "installing pip"
        & $exe -m ensurepip --upgrade
    }
    $pver = (& $exe --version 2>&1).Split(" ")[1]
    $mver = (& $exe -m pip --version 2>$null).Split(" ")[1]
    Log "python $pver / pip $mver"
}

function Setup-Maturin {
    $pip = Get-Command python -ErrorAction SilentlyContinue
    if (-not $pip) { Die "python is missing: list python in -Tools before maturin" }
    $shown = & python -m pip show maturin 2>$null | Select-String "^Version: (.*)"
    if ($shown -and $shown.Matches.Groups[1].Value -eq $MaturinVersion) {
        Log "maturin $MaturinVersion (already present)"
        return
    }
    Log "installing maturin==$MaturinVersion"
    & python -m pip install --quiet "maturin==$MaturinVersion"
    if ($LASTEXITCODE -ne 0) { Die "pip install maturin==$MaturinVersion failed" }
    $iv = (& python -m pip show maturin 2>$null | Select-String "^Version: (.*)").Matches.Groups[1].Value
    Log "maturin $iv"
}

function Setup-Java {
    $javac = Get-Command javac -ErrorAction SilentlyContinue
    $have = ""
    if ($javac) {
        $have = ((& javac -version 2>&1) -replace '.* ', '' -split '\.')[0]
        if ($have -eq $JavaVersion) {
            Log "javac $(& javac -version 2>&1) (already present)"
        } else {
            Log "javac major is $have, want $JavaVersion; installing beside it"
            $have = ""
        }
    }
    if (-not $have) {
        Log "installing temurin $JavaVersion (windows/x64)"
        $tmp = Join-Path $env:TEMP "jdk.zip"
        Fetch "https://api.adoptium.net/v3/binary/latest/$JavaVersion/ga/windows/x64/jdk/hotspot/normal/eclipse" $tmp
        $dest = Join-Path $ToolsRoot "temurin-$JavaVersion"
        if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }
        New-Item -ItemType Directory -Path $dest -Force | Out-Null
        Expand-Archive -Path $tmp -DestinationPath $dest -Force
        Remove-Item $tmp -Force
        $inner = Get-ChildItem -Path $dest -Directory | Select-Object -First 1
        if ($inner) { $dest = $inner.FullName }
        Set-StepEnv "JAVA_HOME" $dest
        Add-ToPath (Join-Path $dest "bin")
        Log "javac $(& javac -version 2>&1)"
    } elseif (-not $env:JAVA_HOME) {
        Set-StepEnv "JAVA_HOME" (Split-Path (Split-Path $javac.Source))
    }
    $mvn = Get-Command mvn -ErrorAction SilentlyContinue
    if (-not $mvn) {
        Log "installing maven $MavenVersion"
        $tmp = Join-Path $env:TEMP "maven.zip"
        Fetch "https://archive.apache.org/dist/maven/maven-3/$MavenVersion/binaries/apache-maven-$MavenVersion-bin.zip" $tmp
        $mdest = Join-Path $ToolsRoot "apache-maven-$MavenVersion"
        if (Test-Path $mdest) { Remove-Item $mdest -Recurse -Force }
        New-Item -ItemType Directory -Path $ToolsRoot -Force | Out-Null
        Expand-Archive -Path $tmp -DestinationPath $ToolsRoot -Force
        Remove-Item $tmp -Force
        Add-ToPath (Join-Path $mdest "bin")
    }
    $mv = ((& mvn --version 2>$null | Select-Object -First 1) -replace '.*Maven ', '' -split ' ')[0]
    Log "mvn $mv"
}

function Setup-Dotnet {
    $dotnet = Get-Command dotnet -ErrorAction SilentlyContinue
    if ($dotnet) {
        $sdks = & dotnet --list-sdks 2>$null
        if ($sdks -match "^$DotnetVersion\.") {
            Log "dotnet $(& dotnet --version) (already present)"
            return
        }
    }
    Log "installing dotnet $DotnetVersion"
    $tmp = Join-Path $env:TEMP "dotnet-install.ps1"
    Fetch "https://dot.net/v1/dotnet-install.ps1" $tmp
    & $tmp -Channel "$DotnetVersion.0" -InstallDir (Join-Path $env:USERPROFILE ".dotnet")
    Remove-Item $tmp -Force
    Set-StepEnv "DOTNET_ROOT" (Join-Path $env:USERPROFILE ".dotnet")
    Add-ToPath (Join-Path $env:USERPROFILE ".dotnet")
    Log "dotnet $(& dotnet --version)"
}

function Setup-Wasm {
    $cargo = Get-Command cargo -ErrorAction SilentlyContinue
    if (-not $cargo) { Die "cargo is missing: list rust in -Tools before wasm" }
    $pack = Get-Command wasm-pack -ErrorAction SilentlyContinue
    if (-not $pack) {
        Log "installing wasm-pack"
        & cargo install wasm-pack --locked
        if ($LASTEXITCODE -ne 0) { Die "cargo install wasm-pack failed" }
    }
    if (-not (Test-Path "Cargo.lock")) { Die "wasm: Cargo.lock is missing: run the setup from the repository root" }
    $lock = Get-Content -Raw "Cargo.lock"
    $m = [regex]::Match($lock, '\[\[package\]\]\r?\nname = "wasm-bindgen"\r?\nversion = "([^"]+)"')
    if (-not $m.Success) { Die "wasm: wasm-bindgen not found in Cargo.lock" }
    $ver = $m.Groups[1].Value
    $bindgen = Get-Command wasm-bindgen -ErrorAction SilentlyContinue
    if ($bindgen -and ((& wasm-bindgen --version).Split(" ")[1] -eq $ver)) {
        Log "wasm-bindgen-cli $ver (already present)"
    } else {
        Log "installing wasm-bindgen-cli $ver (from Cargo.lock)"
        & cargo install wasm-bindgen-cli --version $ver --locked
        if ($LASTEXITCODE -ne 0) { Die "cargo install wasm-bindgen-cli $ver failed" }
    }
    $pver = (& wasm-pack --version).Split(" ")[1]
    $bver = (& wasm-bindgen --version).Split(" ")[1]
    Log "wasm-pack $pver / wasm-bindgen $bver"
}

# The installed @napi-rs/cli version, from npm: the 2.x CLI has no --version
# option, and its usage error (exit 1) became the step's exit code (smoke
# run 37331673176).
function Get-NapiVersion {
    $json = & npm ls -g --depth=0 --json "@napi-rs/cli" 2>$null | Out-String
    $global:LASTEXITCODE = 0
    try { return ($json | ConvertFrom-Json).dependencies.'@napi-rs/cli'.version } catch { return $null }
}

function Setup-Napi {
    $node = Get-Command node -ErrorAction SilentlyContinue
    if (-not $node) { Die "node is missing: list node in -Tools before napi" }
    $pin = $NapiVersion
    if (Test-Path "crates/pdf-node/package.json") {
        $pkg = Get-Content -Raw "crates/pdf-node/package.json" | ConvertFrom-Json
        $want = $pkg.devDependencies.'@napi-rs/cli'
        if ($want) { $pin = ($want -replace '^[\^~>=< ]*', '') }
    }
    if ((Get-NapiVersion) -eq $pin) {
        Log "@napi-rs/cli $pin (already present)"
        return
    }
    Log "installing @napi-rs/cli@$pin"
    & npm install -g "@napi-rs/cli@$pin"
    if ($LASTEXITCODE -ne 0) { Die "npm install -g @napi-rs/cli@$pin failed" }
    $have = Get-NapiVersion
    if ($have -ne $pin) { Die "@napi-rs/cli: npm reports '$have' after installing $pin" }
    Log "@napi-rs/cli $have"
}

# The ziglang wheel ships no `zig` console script on any platform (its
# only entry point is `python-zig`): the compiler lives inside the
# package dir and runs as `python -m ziglang`. The version log's
# `& zig version` aborted setup with "The term 'zig' is not recognized"
# on the win-x64 natives leg (build 2), so both the keep-check and the
# version log probe the module entry point instead. Never throws: a
# probe that cannot answer reports nothing, and the caller decides.
function Get-ZigVersion {
    try {
        $out = & python -m ziglang version 2>$null
        if ($LASTEXITCODE -eq 0 -and $out) {
            return ($out | Select-Object -First 1)
        }
    } catch { }
    return $null
}

# The cargo-zigbuild half of the same log line. The version log never
# fails the setup: an absent binary answers "unknown" instead of
# throwing under Stop.
function Get-CargoZigbuildVersion {
    try {
        $out = & cargo-zigbuild --version 2>$null
        if ($LASTEXITCODE -eq 0 -and $out) {
            return (($out | Select-Object -First 1).Split(' ')[1])
        }
    } catch { }
    return "unknown"
}

function Setup-Zig {
    $python = Get-Command python -ErrorAction SilentlyContinue
    if (-not $python) { Die "python is missing: list python in -Tools before zig" }
    $zb = Get-Command cargo-zigbuild -ErrorAction SilentlyContinue
    $zv = Get-ZigVersion
    if ($zb -and $zv) {
        Log "cargo-zigbuild $(Get-CargoZigbuildVersion) / zig $zv (already present)"
        return
    }
    Log "installing cargo-zigbuild ziglang"
    & python -m pip install --quiet cargo-zigbuild ziglang
    if ($LASTEXITCODE -ne 0) { Die "pip install cargo-zigbuild ziglang failed" }
    $zv = Get-ZigVersion
    if (-not $zv) { $zv = "unknown" }
    Log "cargo-zigbuild $(Get-CargoZigbuildVersion) / zig $zv"
}

function Setup-Docker {
    Log "docker is not installed by this setup on Windows; no release job needs containers there"
}

foreach ($Tool in @("rust", "node", "python", "maturin", "java", "dotnet", "wasm", "napi", "zig", "docker")) {
    if (Want $Tool) {
        $fn = "Setup-" + $Tool.Substring(0, 1).ToUpper() + $Tool.Substring(1)
        & $fn
    }
}

Log "done: $Tools"
# The runner's pwsh wrapper exits with $LASTEXITCODE, which is whatever the
# last native probe above left behind: every real failure has already called
# Die, so reaching this line is success.
exit 0
