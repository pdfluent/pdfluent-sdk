@echo off
rem Copyright (c) 2026 Innovation Trigger B.V.
rem
rem PDFluent is available under two licences, at your option: the GNU AGPLv3, or
rem the PDFluent Commercial Licence. See the LICENSE file in this repository --
rem that file travels with the copy you received, which a URL does not.
rem
rem The MSVC half of the Windows wheel build. Called by windows_wheel_build.ps1
rem with the source directory as its one argument; see §3.2 (c) of
rem docs/release/REGISTRY_1_0_0_RUNBOOK.md.
rem
rem It exists as a .cmd because link.exe and cl.exe are not on PATH on the build
rem host: vcvars64.bat is a batch script, and what it exports has to survive
rem into the same process that runs cargo.
setlocal
if "%~1"=="" ( echo usage: windows_wheel_build.cmd ^<source-directory^> & exit /b 2 )

set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if not exist "%VSWHERE%" ( echo no vswhere.exe: Visual Studio Build Tools are not installed & exit /b 1 )
set "VSPATH="
for /f "usebackq tokens=*" %%i in (`"%VSWHERE%" -latest -products * -property installationPath`) do set "VSPATH=%%i"
if not defined VSPATH ( echo vswhere found no Visual Studio installation & exit /b 1 )

call "%VSPATH%\VC\Auxiliary\Build\vcvars64.bat" >nul || exit /b 1
cd /d "%~1" || exit /b 1
maturin build --release --locked -m crates\pdf-python\Cargo.toml --out dist
exit /b %ERRORLEVEL%
