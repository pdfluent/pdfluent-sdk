# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# Build the Windows x64 wheel of the Python binding on the Windows build host.
# §3.2 (c) of docs/release/REGISTRY_1_0_0_RUNBOOK.md is the route; this is the
# half that runs on that machine. Copy it and windows_wheel_build.cmd over, put
# a `git archive` of the commit beside them, and run it with `-File`.
#
# WHY IT ENFORCES THE FLOOR RATHER THAN REPORTING IT
#
# The build host is shared: the editor's Windows release, a Granola integration
# and an archive live on the same C:, and it is the only drive. A build that
# stops at the floor costs an hour; a full C: costs somebody else their day. So
# the free space is read every twenty seconds against a floor, and below it the
# build is killed by the pid this script started -- never by name, because the
# names on that machine are not all ours.
#
# WHY THE EXIT CODE IS READ AFTER WaitForExit()
#
# `Start-Process -PassThru` hands back a process object whose `ExitCode` is
# empty until `WaitForExit()` has been called on that object, even once
# `HasExited` is true. The first run of this build printed an empty exit code
# next to a wheel that was in fact fine -- a build that failed would have
# printed the same thing.
[CmdletBinding()]
param(
    # The `git archive` of the commit to build, already on this machine.
    [string] $Archive = 'C:\pdfluent-wheel-src.tar.gz',
    # Where it is unpacked and built. Removed and recreated; do not point this
    # at a checkout you want to keep.
    [string] $Root = 'C:\pdfluent-wheel-build',
    # Stop the build when C: has less than this much free, in GB.
    [double] $FloorGB = 8.0
)

$ErrorActionPreference = 'Stop'

function Get-FreeGB {
    [math]::Round((Get-CimInstance Win32_LogicalDisk -Filter "DeviceID='C:'").FreeSpace / 1GB, 2)
}

$cmd = Join-Path $PSScriptRoot 'windows_wheel_build.cmd'
if (-not (Test-Path $cmd)) { throw "windows_wheel_build.cmd is not beside this script ($cmd)" }
if (-not (Test-Path $Archive)) { throw "no source archive at $Archive" }

$before = Get-FreeGB
Write-Output ("free-before-gb " + $before)
# Four GB above the floor, so a build that cannot finish does not start and
# leave a half-written target behind on a disk somebody else is using.
if ($before -lt ($FloorGB + 4)) { throw ("C: has only " + $before + " GB free; refusing to start") }

if (Test-Path $Root) { Remove-Item $Root -Recurse -Force }
New-Item -ItemType Directory -Path $Root | Out-Null
tar -xzf $Archive -C $Root
Write-Output ("free-after-extract-gb " + (Get-FreeGB))

# cargo writes its progress to stderr, so the interesting half of the build is
# in build.err. Both are kept: a reader who opens the wrong one finds it empty
# rather than finds nothing at all.
$proc = Start-Process -FilePath 'cmd.exe' -ArgumentList '/c', "`"$cmd`"", "`"$Root`"" `
    -RedirectStandardOutput (Join-Path $Root 'build.log') `
    -RedirectStandardError (Join-Path $Root 'build.err') `
    -NoNewWindow -PassThru
Write-Output ("build-pid " + $proc.Id)

$lowest = $before
while (-not $proc.HasExited) {
    Start-Sleep -Seconds 20
    $free = Get-FreeGB
    if ($free -lt $lowest) { $lowest = $free }
    if ($free -lt $FloorGB) {
        Write-Output ("FLOOR-HIT " + $free)
        & taskkill /T /F /PID $proc.Id | Out-Null
        Write-Output ("lowest-free-gb " + $lowest)
        throw ("C: fell below " + $FloorGB + " GB; the build was stopped")
    }
}
$proc.WaitForExit()
$code = $proc.ExitCode
Write-Output ("exit-code " + $code)
Write-Output ("lowest-free-gb " + $lowest)
Write-Output ("free-after-gb " + (Get-FreeGB))

$dist = Join-Path $Root 'dist'
if (Test-Path $dist) {
    Get-ChildItem $dist -Filter *.whl | ForEach-Object {
        "wheel {0} {1}" -f $_.Name, $_.Length
        "sha256 {0}" -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash
    }
}

if ($code -ne 0) { throw ("maturin exited " + $code + "; read " + (Join-Path $Root 'build.err')) }
