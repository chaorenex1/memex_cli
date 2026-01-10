#!/bin/bash
set -e

if [ -z "$1" ]; then
  echo "Usage: ./scripts/bump-version.sh <version>"
  echo "Example: ./scripts/bump-version.sh 1.2.0"
  exit 1
fi

VERSION="$1"

echo "Updating version to $VERSION..."

# Update cli/Cargo.toml (only the package version, not dependencies)
sed -i.bak '0,/^version = /s/^version = ".*"/version = "'"$VERSION"'"/' cli/Cargo.toml && rm cli/Cargo.toml.bak
# Update core/Cargo.toml (only the package version, not dependencies)
sed -i.bak '0,/^version = /s/^version = ".*"/version = "'"$VERSION"'"/' core/Cargo.toml && rm core/Cargo.toml.bak
# Update plugins/Cargo.toml (only the package version, not dependencies)
sed -i.bak '0,/^version = /s/^version = ".*"/version = "'"$VERSION"'"/' plugins/Cargo.toml && rm plugins/Cargo.toml.bak

# Update all npm package.json files
for dir in npm/memex-cli npm/darwin-arm64 npm/darwin-x64 npm/linux-x64 npm/win32-x64; do
  jq --arg v "$VERSION" '.version = $v' "$dir/package.json" > "$dir/package.json.tmp"
  mv "$dir/package.json.tmp" "$dir/package.json"
done

# Update optionalDependencies in main npm package
jq --arg v "$VERSION" '.optionalDependencies = (.optionalDependencies | to_entries | map(.value = $v) | from_entries)' \
  npm/memex-cli/package.json > npm/memex-cli/package.json.tmp
mv npm/memex-cli/package.json.tmp npm/memex-cli/package.json

echo "Updated files:"
echo "  - cli/Cargo.toml"
echo "  - npm/memex-cli/package.json"
echo "  - npm/darwin-arm64/package.json"
echo "  - npm/darwin-x64/package.json"
echo "  - npm/linux-x64/package.json"
echo "  - npm/win32-x64/package.json"
echo ""
echo "Done! Version updated to $VERSION"
echo "Next steps:"
echo "  git add -A"
echo "  git commit -m \"chore: bump version to $VERSION\""
echo "  git tag v$VERSION"
echo "  git push && git push --tags"
