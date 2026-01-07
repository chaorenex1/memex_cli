@echo off
setlocal EnableDelayedExpansion

:: Configuration
set "REPO=chaorenex1/memex-cli"
set "NAME=memex-cli"
set "INSTALL_DIR=%USERPROFILE%\.local\bin"

:: Filename for Windows: memex-cli-x86_64-pc-windows-msvc.zip
set "FILENAME=memex-cli-x86_64-pc-windows-msvc.zip"

title %NAME% Installer
cls
echo.
echo === %NAME% Installer ===
echo.
echo [INFO] System: Windows x86_64
echo [INFO] Target: %FILENAME%

:: Create dirs
set "TMP=%TEMP%\memex_%RANDOM%"
mkdir "%TMP%" 2>nul
mkdir "%INSTALL_DIR%" 2>nul

:: Get latest version
echo [INFO] Fetching latest release...
set "API=https://api.github.com/repos/%REPO%/releases/latest"

for /f "tokens=*" %%v in ('powershell -Command "(Invoke-RestMethod -Uri '%API%').tag_name" 2^>nul') do set "VERSION=%%v"

if not defined VERSION (
    echo [ERROR] Cannot fetch release info
    goto :fail
)

echo [INFO] Version: %VERSION%

:: Download
set "URL=https://github.com/%REPO%/releases/download/%VERSION%/%FILENAME%"
echo [INFO] Downloading: %URL%

powershell -Command "Invoke-WebRequest -Uri '%URL%' -OutFile '%TMP%\%FILENAME%'" 2>nul
if not exist "%TMP%\%FILENAME%" (
    echo [ERROR] Download failed
    goto :fail
)
echo [OK] Download complete

:: Extract
echo [INFO] Extracting...
powershell -Command "Expand-Archive -Path '%TMP%\%FILENAME%' -DestinationPath '%TMP%' -Force" 2>nul
echo [OK] Extraction complete

:: Find binary
set "BIN="
for /r "%TMP%" %%f in (memex-cli.exe memex.exe) do (
    if exist "%%f" (
        set "BIN=%%f"
        goto :install
    )
)

:: Try without .exe
for /r "%TMP%" %%f in (memex-cli memex) do (
    if exist "%%f" (
        set "BIN=%%f"
        goto :install
    )
)

echo [ERROR] Binary not found
echo [INFO] Extracted files:
dir /s /b "%TMP%" 2>nul
goto :fail

:install
echo [INFO] Found: %BIN%

set "TARGET=%INSTALL_DIR%\%NAME%.exe"
if exist "%TARGET%" echo [WARN] Overwriting existing version

copy /y "%BIN%" "%TARGET%" >nul
if %errorlevel% neq 0 (
    echo [ERROR] Copy failed
    goto :fail
)
echo [OK] Installed: %TARGET%

:: Install memex-env scripts (optional)
echo.
echo [INFO] Installing memex-env scripts...
set "SCRIPTS_URL=https://github.com/%REPO%/releases/download/%VERSION%/memex-env-scripts.zip"
set "SCRIPTS_ARCHIVE=%TMP%\memex-env-scripts.zip"

powershell -Command "try { Invoke-WebRequest -Uri '%SCRIPTS_URL%' -OutFile '%SCRIPTS_ARCHIVE%' -ErrorAction Stop } catch { exit 1 }" 2>nul
if %errorlevel% equ 0 (
    echo [INFO] Extracting memex-env scripts...
    powershell -Command "Expand-Archive -Path '%SCRIPTS_ARCHIVE%' -DestinationPath '%TMP%' -Force" 2>nul
    if %errorlevel% equ 0 (
        set "INSTALLED_SCRIPTS=0"
        for %%f in ("%TMP%\scripts\memex-env.*") do (
            if exist "%%f" (
                copy /y "%%f" "%INSTALL_DIR%\" >nul
                echo [OK] Installed: %INSTALL_DIR%\%%~nxf
                set /a INSTALLED_SCRIPTS+=1
            )
        )
        if !INSTALLED_SCRIPTS! equ 0 (
            echo [WARN] No memex-env scripts found in archive
        )
    ) else (
        echo [WARN] Failed to extract memex-env scripts
        echo [INFO] Continuing without memex-env scripts...
    )
) else (
    echo [INFO] memex-env scripts not available in this release
    echo [INFO] Continuing with main installation...
)

:: Update PATH
echo %PATH% | findstr /i "%INSTALL_DIR%" >nul
if %errorlevel% neq 0 (
    powershell -Command "[Environment]::SetEnvironmentVariable('Path', [Environment]::GetEnvironmentVariable('Path','User') + ';%INSTALL_DIR%', 'User')" 2>nul
    echo [WARN] Added to PATH - restart terminal
) else (
    echo [OK] %INSTALL_DIR% already in PATH
)

:: Cleanup
rd /s /q "%TMP%" 2>nul

echo.
echo === Installation Complete ===
echo.
echo Run: %NAME% --help
echo.

"%TARGET%" --help 2>nul
goto :end

:fail
rd /s /q "%TMP%" 2>nul
echo.
echo Installation failed.

:end
echo.
pause
