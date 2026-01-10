param(
    [Parameter(Mandatory=$true)]
    [string]$Version
)

$ErrorActionPreference = "Stop"

Write-Host "Updating version to $Version..."

# Update cli/Cargo.toml (only first occurrence - package version)
$cliCargoPath = "cli/Cargo.toml"
$lines = Get-Content $cliCargoPath
$updated = $false
$newLines = $lines | ForEach-Object {
    if (-not $updated -and $_ -match '^version = ') {
        $updated = $true
        "version = `"$Version`""
    } else {
        $_
    }
}
$newLines | Set-Content $cliCargoPath -Encoding UTF8

# Update core/Cargo.toml (only first occurrence - package version)
$coreCargoPath = "core/Cargo.toml"
$lines = Get-Content $coreCargoPath
$updated = $false
$newLines = $lines | ForEach-Object {
    if (-not $updated -and $_ -match '^version = ') {
        $updated = $true
        "version = `"$Version`""
    } else {
        $_
    }
}
$newLines | Set-Content $coreCargoPath -Encoding UTF8

# Update plugins/Cargo.toml (only first occurrence - package version)
$pluginsCargoPath = "plugins/Cargo.toml"
$lines = Get-Content $pluginsCargoPath
$updated = $false
$newLines = $lines | ForEach-Object {
    if (-not $updated -and $_ -match '^version = ') {
        $updated = $true
        "version = `"$Version`""
    } else {
        $_
    }
}
$newLines | Set-Content $pluginsCargoPath -Encoding UTF8

# Update all npm package.json files
$npmDirs = @("npm/memex-cli", "npm/darwin-arm64", "npm/darwin-x64", "npm/linux-x64", "npm/win32-x64")

foreach ($dir in $npmDirs) {
    $pkgPath = "$dir/package.json"
    $pkg = Get-Content $pkgPath -Raw | ConvertFrom-Json
    $pkg.version = $Version
    $pkg | ConvertTo-Json -Depth 10 | Set-Content $pkgPath -Encoding UTF8
}

# Update optionalDependencies in main npm package
$mainPkgPath = "npm/memex-cli/package.json"
$mainPkg = Get-Content $mainPkgPath -Raw | ConvertFrom-Json
$mainPkg.optionalDependencies.PSObject.Properties | ForEach-Object {
    $_.Value = $Version
}
$mainPkg | ConvertTo-Json -Depth 10 | Set-Content $mainPkgPath -Encoding UTF8

Write-Host ""
Write-Host "Updated files:" -ForegroundColor Green
Write-Host "  - cli/Cargo.toml"
Write-Host "  - npm/memex-cli/package.json"
Write-Host "  - npm/darwin-arm64/package.json"
Write-Host "  - npm/darwin-x64/package.json"
Write-Host "  - npm/linux-x64/package.json"
Write-Host "  - npm/win32-x64/package.json"
Write-Host ""
Write-Host "Done! Version updated to $Version" -ForegroundColor Green
Write-Host ""
Write-Host "Next steps:" -ForegroundColor Yellow
Write-Host "  git add -A"
Write-Host "  git commit -m `"chore: bump version to $Version`""
Write-Host "  git tag v$Version"
Write-Host "  git push && git push --tags"
