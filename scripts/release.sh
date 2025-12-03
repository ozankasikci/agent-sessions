#!/bin/bash
set -e

# Release script for Agent Sessions
# This script builds, signs, notarizes, and creates properly styled DMGs for both architectures

# Configuration
APP_NAME="Agent Sessions"
BUNDLE_ID="com.claude-sessions-viewer"
SIGNING_IDENTITY="Developer ID Application: Ozan Kasikci (5K5S7L7L7M)"
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TAURI_DIR="$PROJECT_ROOT/src-tauri"
BUNDLE_DMG_SCRIPT="$TAURI_DIR/target/aarch64-apple-darwin/release/bundle/dmg/bundle_dmg.sh"

# Get version from tauri.conf.json
VERSION=$(grep '"version"' "$TAURI_DIR/tauri.conf.json" | head -1 | sed 's/.*"version": "\([^"]*\)".*/\1/')

echo "=== Agent Sessions Release Script ==="
echo "Version: $VERSION"
echo "Project root: $PROJECT_ROOT"
echo ""

# Check for required credentials
if [ -z "$APPLE_ID" ] || [ -z "$APPLE_PASSWORD" ] || [ -z "$APPLE_TEAM_ID" ]; then
    echo "Error: Missing Apple credentials. Please set:"
    echo "  APPLE_ID - Your Apple ID email"
    echo "  APPLE_PASSWORD - App-specific password"
    echo "  APPLE_TEAM_ID - Your Team ID"
    exit 1
fi

# Function to build for a specific architecture
build_arch() {
    local arch=$1
    local target=$2

    echo "=== Building for $arch ($target) ==="
    cd "$PROJECT_ROOT"
    npm run tauri build -- --target "$target"
    echo "Build complete for $arch"
}

# Function to create styled DMG using Tauri's bundle_dmg.sh
create_dmg() {
    local arch=$1
    local target=$2
    local dmg_name="AgentSessions_${VERSION}_${arch}.dmg"
    local bundle_dir="$TAURI_DIR/target/$target/release/bundle"
    local app_path="$bundle_dir/macos/${APP_NAME}.app"
    local dmg_path="$bundle_dir/dmg/$dmg_name"
    local output_dir="$PROJECT_ROOT/release"

    echo "=== Creating DMG for $arch ==="

    # Ensure the bundle_dmg.sh script exists
    if [ ! -f "$BUNDLE_DMG_SCRIPT" ]; then
        echo "Error: bundle_dmg.sh not found at $BUNDLE_DMG_SCRIPT"
        echo "Run a build first to generate the script"
        exit 1
    fi

    # Create release output directory
    mkdir -p "$output_dir"

    # Run the Tauri DMG bundler script
    # This creates a properly styled DMG with volume icon, Applications symlink, and Finder layout
    cd "$bundle_dir/dmg"

    # Remove old DMG if exists
    rm -f "$dmg_name"

    # The bundle_dmg.sh script expects to be run from the dmg directory
    # and will create the DMG with proper styling
    bash "$BUNDLE_DMG_SCRIPT" "$app_path" "$dmg_name"

    echo "DMG created at $dmg_path"

    # Copy to release directory
    cp "$dmg_path" "$output_dir/"
    echo "Copied to $output_dir/$dmg_name"
}

# Function to sign DMG
sign_dmg() {
    local arch=$1
    local dmg_name="AgentSessions_${VERSION}_${arch}.dmg"
    local dmg_path="$PROJECT_ROOT/release/$dmg_name"

    echo "=== Signing DMG for $arch ==="
    codesign --force --sign "$SIGNING_IDENTITY" --timestamp --options runtime "$dmg_path"
    echo "Signed: $dmg_path"
}

# Function to notarize DMG
notarize_dmg() {
    local arch=$1
    local dmg_name="AgentSessions_${VERSION}_${arch}.dmg"
    local dmg_path="$PROJECT_ROOT/release/$dmg_name"

    echo "=== Notarizing DMG for $arch ==="
    xcrun notarytool submit "$dmg_path" \
        --apple-id "$APPLE_ID" \
        --password "$APPLE_PASSWORD" \
        --team-id "$APPLE_TEAM_ID" \
        --wait

    echo "=== Stapling notarization ticket for $arch ==="
    xcrun stapler staple "$dmg_path"
    echo "Notarization complete for $arch"
}

# Function to calculate SHA256
calc_sha256() {
    local arch=$1
    local dmg_name="AgentSessions_${VERSION}_${arch}.dmg"
    local dmg_path="$PROJECT_ROOT/release/$dmg_name"

    shasum -a 256 "$dmg_path" | awk '{print $1}'
}

# Main release process
main() {
    local skip_build=false
    local arch_filter=""

    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            --skip-build)
                skip_build=true
                shift
                ;;
            --arch)
                arch_filter=$2
                shift 2
                ;;
            --help)
                echo "Usage: $0 [options]"
                echo ""
                echo "Options:"
                echo "  --skip-build    Skip the build step (use existing builds)"
                echo "  --arch <arch>   Build only for specific arch (aarch64 or x64)"
                echo "  --help          Show this help message"
                exit 0
                ;;
            *)
                echo "Unknown option: $1"
                exit 1
                ;;
        esac
    done

    # Determine which architectures to build
    local archs=()
    if [ -z "$arch_filter" ]; then
        archs=("aarch64" "x64")
    else
        archs=("$arch_filter")
    fi

    # Helper function to map arch to target
    get_target() {
        case "$1" in
            aarch64) echo "aarch64-apple-darwin" ;;
            x64) echo "x86_64-apple-darwin" ;;
        esac
    }

    # Build
    if [ "$skip_build" = false ]; then
        for arch in "${archs[@]}"; do
            build_arch "$arch" "$(get_target "$arch")"
        done
    fi

    # Create DMGs
    for arch in "${archs[@]}"; do
        create_dmg "$arch" "$(get_target "$arch")"
    done

    # Sign DMGs
    for arch in "${archs[@]}"; do
        sign_dmg "$arch"
    done

    # Notarize DMGs
    for arch in "${archs[@]}"; do
        notarize_dmg "$arch"
    done

    # Print summary
    echo ""
    echo "=== Release Complete ==="
    echo "Version: $VERSION"
    echo ""
    echo "DMG files in $PROJECT_ROOT/release/:"
    for arch in "${archs[@]}"; do
        local dmg_name="AgentSessions_${VERSION}_${arch}.dmg"
        local sha=$(calc_sha256 "$arch")
        echo "  $dmg_name"
        echo "    SHA256: $sha"
    done

    echo ""
    echo "=== Homebrew Cask Update ==="
    echo "Update homebrew-tap/Casks/agent-sessions.rb with:"
    echo ""
    for arch in "${archs[@]}"; do
        local sha=$(calc_sha256 "$arch")
        if [ "$arch" = "aarch64" ]; then
            echo "  on_arm do"
            echo "    sha256 \"$sha\""
        else
            echo "  on_intel do"
            echo "    sha256 \"$sha\""
        fi
    done

    echo ""
    echo "=== GitHub Release ==="
    echo "Create a new release at:"
    echo "  https://github.com/ozankasikci/agent-sessions/releases/new"
    echo ""
    echo "Tag: v$VERSION"
    echo "Upload the DMG files from $PROJECT_ROOT/release/"
}

main "$@"
