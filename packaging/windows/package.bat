@echo off
rem Build and package Strix for Windows: double-click this file, or run it from a terminal.
rem It runs packaging\windows\package.ps1 (requirements there) from the repository root.
rem
rem   package.bat                      x64 (default)
rem   package.bat -Arch x86            or arm64 (needs the MSVC ARM64 build tools)
rem   package.bat -Arch x64 -SkipBuild repackage the binaries already built
rem
rem The packages (.msi, -portable.zip, -portable.exe) go to %DIST% when it is set, otherwise to
rem dist\release in the repository root, the same folder package.ps1 and docs/releasing.md use.
setlocal

for %%I in ("%~dp0..\..") do set "ROOT=%%~fI"
cd /d "%ROOT%" || goto :failed
if defined DIST (set "OUT=%DIST%") else (set "OUT=%ROOT%\dist\release")

rem PowerShell 7 when installed, else Windows PowerShell; Bypass lets the unsigned script run.
set "PS=powershell"
where pwsh >nul 2>nul && set "PS=pwsh"

%PS% -NoProfile -ExecutionPolicy Bypass -File "%~dp0package.ps1" %*
if errorlevel 1 goto :failed

echo.
echo Packages ready in %OUT%:
dir /b "%OUT%\strix-*"
call :pause_if_double_clicked open
exit /b 0

:failed
echo.
echo Packaging failed: see the messages above.
call :pause_if_double_clicked
exit /b 1

rem A double click runs "cmd /c ...": keep the window open (and open the folder) only then.
:pause_if_double_clicked
setlocal EnableDelayedExpansion
set "CMDLINE=!cmdcmdline!"
if /i "!CMDLINE:/c =!"=="!CMDLINE!" exit /b 0
if "%~1"=="open" start "" "%OUT%"
pause
exit /b 0
