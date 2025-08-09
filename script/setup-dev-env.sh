#!/bin/bash

# A script to set up a Rust development environment for creating
# cross-platform GStreamer/GTK4 apps on Debian-based Linux (for WSL2).

# --- Stop on any error ---
set -e

echo "--- [1/5] Updating system packages ---"
sudo apt update
sudo apt upgrade -y

echo "--- [2/5] Installing essential build tools and system libraries ---"
# build-essential: Compilers (gcc), make, etc.
# pkg-config: Helps Rust's build scripts find C libraries. CRITICAL.
# libgtk-4-dev: Development files for GTK4.
# libgstreamer1.0-dev: Development files for GStreamer core.
# libgstreamer-plugins-base1.0-dev: Dev files for essential plugins.
# gstreamer1.0-plugins-*: The actual plugin packages needed at runtime.
# gstreamer1.0-libav: FFmpeg integration for wide codec support.
# libssl-dev, libclang-dev: Common dependencies for many Rust crates.
sudo apt install -y \
    build-essential \
    pkg-config \
    libgtk-4-dev \
    libgstreamer1.0-dev \
    libgstreamer-plugins-base1.0-dev \
    gstreamer1.0-plugins-good \
    gstreamer1.0-plugins-bad \
    gstreamer1.0-plugins-ugly \
    gstreamer1.0-libav \
    libssl-dev \
    libclang-dev

echo "--- [3/5] Installing Rust via rustup ---"
# This checks if rustup is already installed to avoid re-running.
if ! command -v rustup &> /dev/null
then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # Add cargo to the current shell's PATH
    source "$HOME/.cargo/env"
else
    echo "Rust is already installed. Updating..."
    rustup update
fi

echo "--- [4/5] Installing Windows cross-compilation toolchain ---"
# mingw-w64: The GNU toolchain that can create Windows executables.
sudo apt install -y mingw-w64

echo "--- [5/5] Configuring Rust for Windows cross-compilation ---"
# Add the Windows target to the Rust toolchain
rustup target add x86_64-pc-windows-gnu

# Create/update the cargo config file to specify the windows linker
mkdir -p "$HOME/.cargo"
cat > "$HOME/.cargo/config.toml" <<'EOF'
# This file is for configuring cargo.
# Add more configurations as needed.

# Set the linker for the Windows GNU target.
[target.x86_64-pc-windows-gnu]
linker = "x86_64-w64-mingw32-gcc"
ar = "x86_64-w64-mingw32-ar"
EOF

echo ""
echo "--- ✅ All Done! ---"
echo "Your environment is now configured."
echo "Close and reopen your terminal for all changes to take effect, or run:"
echo "source \"\$HOME/.cargo/env\""
