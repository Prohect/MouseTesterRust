# Build Requirements for Linux

This document describes the system dependencies required to build MouseTesterRust on Linux.

## System Dependencies

### libfontconfig1-dev

The project uses GUI libraries (eframe/egui) that depend on fontconfig. On Linux systems, you need to install the fontconfig development files before building.

**Installation on Debian/Ubuntu-based systems:**

```bash
sudo apt-get update
sudo apt-get install -y libfontconfig1-dev
```

**Installation on Fedora/RHEL-based systems:**

```bash
sudo dnf install fontconfig-devel
```

**Installation on Arch Linux:**

```bash
sudo pacman -S fontconfig
```

## Building the Project

After installing the required system dependencies, you can build the project:

```bash
cargo build
```

Or for a release build:

```bash
cargo build --release
```

## Troubleshooting

### Error: "The system library `fontconfig` required by crate `yeslogic-fontconfig-sys` was not found"

If you see this error during build:

```
thread 'main' panicked at /home/runner/.cargo/registry/src/.../yeslogic-fontconfig-sys-.../build.rs:8:48:
called `Result::unwrap()` on an `Err` value: "...
The system library `fontconfig` required by crate `yeslogic-fontconfig-sys` was not found.
The file `fontconfig.pc` needs to be installed and the PKG_CONFIG_PATH environment variable must contain its parent directory.
```

**Solution:** Install `libfontconfig1-dev` (or equivalent for your distribution) as shown above.

### Verify Installation

After installing fontconfig, you can verify it's available to pkg-config:

```bash
pkg-config --modversion fontconfig
```

This should output the version number of fontconfig if properly installed.

## Platform Notes

- **Windows:** No additional system dependencies required. The project is designed for Windows with USBPcap.
- **Linux:** Requires fontconfig development files as documented above. Note that USB capture functionality requires Windows and USBPcap.
- **macOS:** May require fontconfig via Homebrew: `brew install fontconfig`

## Why is fontconfig needed?

The project uses `eframe` and `egui` for the GUI, which depend on font rendering capabilities. On Linux, this requires the fontconfig library to locate and configure fonts on the system. The `yeslogic-fontconfig-sys` crate provides Rust bindings to fontconfig, which requires the C library headers during compilation.
