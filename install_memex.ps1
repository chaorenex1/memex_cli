#!/usr/bin/env pwsh
# memex-cli Windows Installer

$ErrorActionPreference = "Stop"

# Configuration
$REPO = "chaorenex1/memex-cli"
$NAME = "memex-cli"
$INSTALL_DIR = "$env:USERPROFILE\.memex\bin"

# Helper functions
function Info-Log {
    param([string]$Message)
    Write-Host "[INFO] $Message" -ForegroundColor Cyan
}

function Success-Log {
    param([string]$Message)
    Write-Host "[OK] $Message" -ForegroundColor Green
}

function Warn-Log {
    param([string]$Message)
    Write-Host "[WARN] $Message" -ForegroundColor Yellow
}

function Error-Log {
    param([string]$Message)
    Write-Host "[ERROR] $Message" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "=== $NAME Installer ===" -ForegroundColor Green
Write-Host ""

# Detect OS and architecture
$OS = [System.Environment]::OSVersion.Platform
$ARCH = [System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture

# Map to release naming convention
$TARGET_OS = if ($IsWindows -or $OS -eq "Win32NT") { "pc-windows-msvc" } else { "unknown-linux-gnu" }

$TARGET_ARCH = switch ($ARCH) {
    "X64"   { "x86_64" }
    "Arm64" { "aarch64" }
    default { Error-Log "Unsupported architecture: $ARCH" }
}

# Build filename: memex-cli-{arch}-{os}.tar.gz or .zip for Windows
$EXTENSION = if ($TARGET_OS -match "windows") { "zip" } else { "tar.gz" }
$FILENAME = "memex-cli-${TARGET_ARCH}-${TARGET_OS}.${EXTENSION}"

Info-Log "System: Windows/$ARCH"
Info-Log "Target: $FILENAME"

# Create temp dir
$TMP = Join-Path $env:TEMP "memex-install-$((New-Guid).Guid)"
New-Item -ItemType Directory -Path $TMP -Force | Out-Null

try {
    # Get latest version
    Info-Log "Fetching latest release..."
    $API = "https://api.github.com/repos/$REPO/releases/latest"

    try {
        $Response = Invoke-RestMethod -Uri $API -TimeoutSec 30
        $VERSION = $Response.tag_name
    }
    catch {
        Error-Log "Cannot fetch release info: $_"
    }

    if ([string]::IsNullOrEmpty($VERSION)) {
        Error-Log "Cannot determine version from GitHub API"
    }

    Info-Log "Version: $VERSION"

    # Download
    $URL = "https://github.com/$REPO/releases/download/$VERSION/$FILENAME"
    Info-Log "Downloading: $URL"

    $OutFile = Join-Path $TMP $FILENAME
    try {
        Invoke-WebRequest -Uri $URL -OutFile $OutFile -TimeoutSec 300 -UseBasicParsing
        Success-Log "Download complete"
    }
    catch {
        Error-Log "Download failed: $_"
    }

    # Extract
    Info-Log "Extracting..."
    Push-Location $TMP

    try {
        if ($EXTENSION -eq "zip") {
            Expand-Archive -Path $FILENAME -DestinationPath "." -Force
        } else {
            # Use tar for .tar.gz (available on Windows 10+)
            tar -xzf $FILENAME
        }
        Success-Log "Extraction complete"
    }
    catch {
        Error-Log "Extraction failed: $_"
    }
    finally {
        Pop-Location
    }

    # Find binary
    $Bin = Get-ChildItem -Path $TMP -Recurse -File |
           Where-Object { $_.Name -eq "memex-cli.exe" -or $_.Name -eq "memex-cli" } |
           Select-Object -First 1

    if ($null -eq $Bin) {
        Warn-Log "Extracted files:"
        Get-ChildItem -Path $TMP -Recurse | ForEach-Object { Write-Host "  $($_.FullName)" }
        Error-Log "Binary not found"
    }

    Info-Log "Found: $($Bin.Name)"

    # Install
    New-Item -ItemType Directory -Path $INSTALL_DIR -Force | Out-Null

    $TargetPath = Join-Path $INSTALL_DIR "$NAME.exe"
    if (Test-Path $TargetPath) {
        Warn-Log "Overwriting existing version"
    }

    Copy-Item -Path $Bin.FullName -Destination $TargetPath -Force
    Success-Log "Installed: $TargetPath"

    # Install memex-env scripts (optional)
    Write-Host ""
    Info-Log "Installing memex-env scripts..."
    $SCRIPTS_URL = "https://github.com/$REPO/releases/download/$VERSION/memex-env-scripts.tar.gz"
    $SCRIPTS_ARCHIVE = Join-Path $TMP "memex-env-scripts.tar.gz"

    try {
        Invoke-WebRequest -Uri $SCRIPTS_URL -OutFile $SCRIPTS_ARCHIVE -TimeoutSec 60 -UseBasicParsing
        Push-Location $TMP

        $ExtractDir = Join-Path $TMP "scripts"
        New-Item -ItemType Directory -Path $ExtractDir -Force | Out-Null

        tar -xzf $SCRIPTS_ARCHIVE -C $ExtractDir 2>$null

        # Search recursively (files are in $ExtractDir/scripts/ subdirectory)
        $ScriptFiles = Get-ChildItem -Path $ExtractDir -Recurse -File -Filter "memex-env.*"
        $InstalledCount = 0

        foreach ($Script in $ScriptFiles) {
            $DestPath = Join-Path $INSTALL_DIR $Script.Name
            Copy-Item -Path $Script.FullName -Destination $DestPath -Force
            Success-Log "Installed: $DestPath"
            $InstalledCount++
        }

        if ($InstalledCount -eq 0) {
            Warn-Log "No memex-env scripts found in archive"
        }
    }
    catch {
        Info-Log "memex-env scripts not available in this release"
        Info-Log "Continuing with main installation..."
    }
    finally {
        Pop-Location
    }

    # Update PATH if needed
    $PathEnv = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($PathEnv -notlike "*$INSTALL_DIR*") {
        Write-Host ""
        Warn-Log "Adding $INSTALL_DIR to user PATH..."

        try {
            [Environment]::SetEnvironmentVariable("Path", "$PathEnv;$INSTALL_DIR", "User")
            Info-Log "PATH updated. Restart your terminal for changes to take effect."
        }
        catch {
            Warn-Log "Could not update PATH automatically. Please add '$INSTALL_DIR' to your PATH manually."
        }
    } else {
        Success-Log "$INSTALL_DIR already in PATH"
    }

    # Verify
    Write-Host ""
    Write-Host "=== Installation Complete ===" -ForegroundColor Green
    Write-Host ""
    Write-Host "Run: $NAME --help"
    Write-Host ""

    # Try to show version
    try {
        & $TargetPath --help 2>$null
    }
    catch {
        # Ignore errors from help command
    }
}
finally {
    # Cleanup temp dir
    if (Test-Path $TMP) {
        Remove-Item -Path $TMP -Recurse -Force -ErrorAction SilentlyContinue
    }
}
