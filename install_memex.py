#!/usr/bin/env python3
"""Cross-platform installer for memex-cli from GitHub releases"""

import os, sys, platform, tempfile, shutil, tarfile, zipfile, json, stat, time
from pathlib import Path
from urllib.request import urlopen, Request
from urllib.error import HTTPError

# Configuration
REPO = "chaorenex1/memex-cli"
NAME = "memex-cli"

def download_optional_asset(url, dest, asset_name, max_retries=2):
    """Download optional asset with retry logic"""
    for attempt in range(max_retries):
        try:
            print(f"[INFO] Downloading {asset_name} (attempt {attempt + 1}/{max_retries})...")
            data = urlopen(Request(url, headers={"User-Agent": "installer"}), timeout=30).read()
            dest.write_bytes(data)
            print(f"[OK] {asset_name} downloaded")
            return True
        except HTTPError as e:
            if e.code == 404:
                print(f"[INFO] {asset_name} not available in this release")
                return False
            if attempt == max_retries - 1:
                print(f"[WARN] Failed to download {asset_name}: {e}")
                return False
            time.sleep(2 ** attempt)
        except Exception as e:
            if attempt == max_retries - 1:
                print(f"[WARN] Failed to download {asset_name}: {e}")
                return False
            time.sleep(2 ** attempt)
    return False

def install_memex_env_scripts(tmp_dir, install_dir, version, system):
    """Download and install memex-env scripts (optional)"""
    try:
        # Determine archive format
        ext = "zip" if system == "windows" else "tar.gz"
        filename = f"memex-env-scripts.{ext}"
        url = f"https://github.com/{REPO}/releases/download/{version}/{filename}"

        archive_path = tmp_dir / filename

        # Download scripts archive (optional, non-blocking)
        if not download_optional_asset(url, archive_path, "memex-env scripts"):
            print("[INFO] Continuing without memex-env scripts...")
            return []

        # Extract scripts
        print("[INFO] Extracting memex-env scripts...")
        if ext == "zip":
            with zipfile.ZipFile(archive_path) as z:
                z.extractall(tmp_dir)
        else:
            with tarfile.open(archive_path, "r:gz") as t:
                t.extractall(tmp_dir)

        # Find and install scripts
        scripts_dir = tmp_dir / "scripts"
        if not scripts_dir.exists():
            print("[WARN] Scripts directory not found in archive")
            return []

        installed_scripts = []
        for script in scripts_dir.glob("memex-env.*"):
            target = install_dir / script.name
            shutil.copy2(script, target)

            # Set executable permission for .sh files
            if script.suffix == ".sh" and system != "windows":
                target.chmod(target.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)

            installed_scripts.append(target)
            print(f"[OK] Installed: {target}")

        return installed_scripts

    except Exception as e:
        print(f"[WARN] Failed to install memex-env scripts: {e}")
        print("[INFO] Main program installation will continue...")
        return []

def main():
    # Detect system
    system = platform.system().lower()
    machine = platform.machine().lower()
    
    # Map to release naming convention
    # Files: memex-cli-{arch}-{target}.{ext}
    # - memex-cli-aarch64-apple-darwin.tar.gz (macOS ARM)
    # - memex-cli-x86_64-apple-darwin.tar.gz (macOS Intel)
    # - memex-cli-x86_64-pc-windows-msvc.zip (Windows)
    # - memex-cli-x86_64-unknown-linux-gnu.tar.gz (Linux)
    
    if system == "darwin":
        target = "apple-darwin"
        ext = "tar.gz"
    elif system == "windows":
        target = "pc-windows-msvc"
        ext = "zip"
    else:
        target = "unknown-linux-gnu"
        ext = "tar.gz"
    
    if machine in ("x86_64", "amd64"):
        arch = "x86_64"
    elif machine in ("aarch64", "arm64"):
        arch = "aarch64"
    else:
        sys.exit(f"[ERROR] Unsupported architecture: {machine}")
    
    filename = f"memex-cli-{arch}-{target}.{ext}"
    install_dir = Path.home() / ".local" / "bin"
    exe = ".exe" if system == "windows" else ""
    
    print(f"\n=== {NAME} Installer ===\n")
    print(f"[INFO] System: {system}/{machine}")
    print(f"[INFO] Target: {filename}")
    
    # Get latest version
    print("[INFO] Fetching latest release...")
    api = f"https://api.github.com/repos/{REPO}/releases/latest"
    try:
        data = json.loads(urlopen(Request(api, headers={"User-Agent": "installer"}), timeout=10).read())
        version = data["tag_name"]
    except Exception as e:
        sys.exit(f"[ERROR] Cannot fetch release info: {e}")
    
    print(f"[INFO] Version: {version}")
    
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        
        # Download
        url = f"https://github.com/{REPO}/releases/download/{version}/{filename}"
        print(f"[INFO] Downloading: {url}")
        
        try:
            archive = tmp / filename
            archive.write_bytes(urlopen(Request(url, headers={"User-Agent": "installer"}), timeout=300).read())
            print("[OK] Download complete")
        except Exception as e:
            sys.exit(f"[ERROR] Download failed: {e}")
        
        # Extract
        print("[INFO] Extracting...")
        if filename.endswith(".zip"):
            with zipfile.ZipFile(archive) as z:
                z.extractall(tmp)
        else:
            with tarfile.open(archive, "r:gz") as t:
                t.extractall(tmp)
        print("[OK] Extraction complete")
        
        # Find binary
        binary = None
        for name_pattern in ["memex-cli" + exe, "memex" + exe, "memex-cli", "memex"]:
            for f in tmp.rglob(name_pattern):
                if f.is_file():
                    binary = f
                    break
            if binary:
                break
        
        if not binary:
            print("[WARN] Extracted files:")
            for f in tmp.rglob("*"):
                if f.is_file():
                    print(f"  {f.name}")
            sys.exit("[ERROR] Binary not found")
        
        print(f"[INFO] Found: {binary.name}")
        
        # Install
        install_dir.mkdir(parents=True, exist_ok=True)
        target = install_dir / (NAME + exe)
        
        if target.exists():
            print("[WARN] Overwriting existing version")
        
        shutil.copy2(binary, target)
        if system != "windows":
            target.chmod(target.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)

        print(f"[OK] Installed: {target}")

        # Install memex-env scripts (optional)
        print("\n[INFO] Installing memex-env scripts...")
        install_memex_env_scripts(tmp, install_dir, version, system)

        # Update PATH
        if str(install_dir) not in os.environ.get("PATH", ""):
            if system == "windows":
                import subprocess
                subprocess.run([
                    "powershell", "-Command",
                    f"[Environment]::SetEnvironmentVariable('Path', [Environment]::GetEnvironmentVariable('Path','User') + ';{install_dir}', 'User')"
                ], capture_output=True)
                print("[WARN] Added to PATH - restart terminal")
            else:
                rc = Path.home() / (".zshrc" if "zsh" in os.environ.get("SHELL", "") else ".bashrc")
                with open(rc, "a") as f:
                    f.write(f'\nexport PATH="{install_dir}:$PATH"\n')
                print(f"[WARN] Added to {rc.name} - restart terminal or: source {rc}")
        else:
            print(f"[OK] {install_dir} already in PATH")
    
    # npm install -g memex-cli (optional)
    print("\n[INFO] Installing memex-cli via npm")
    try:
        import subprocess
        subprocess.run(["npm", "install", "-g", "memex-cli"], check=True)
        print("[OK] memex-cli installed via npm")
    except Exception as e:
        print(f"[WARN] npm install failed: {e}")
        print("[INFO] Continuing without npm installation...")
    
    print(f"\n=== Installation Complete ===\n")
    print(f"Run: {NAME} --help\n")
    
    # Show version
    import subprocess
    subprocess.run([str(target), "--help"], capture_output=False)

if __name__ == "__main__":
    main()
