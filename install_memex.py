#!/usr/bin/env python3
"""Cross-platform installer for memex-cli from GitHub releases"""

import os, sys, platform, tempfile, shutil, tarfile, zipfile, json, stat
from pathlib import Path
from urllib.request import urlopen, Request

# Configuration
REPO = "chaorenex1/memex-cli"
NAME = "memex"

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
    
    print(f"\n=== Installation Complete ===\n")
    print(f"Run: {NAME} --help\n")
    
    # Show version
    import subprocess
    subprocess.run([str(target), "--help"], capture_output=False)

if __name__ == "__main__":
    main()
