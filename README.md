# MouseTesterRust

A Rust-based tool for capturing and analyzing USB mouse events on Windows using USBPcap.

## Features

- Capture USB mouse movement data in real-time
- Analyze mouse movement patterns and statistics
- **NEW: Interactive GUI for easy visualization and analysis**
- **NEW: Offline LOD (Level-of-Detail) module for efficient static plotting**
- **NEW: Refactored MouseMoveEvent with precise pcap timestamps**
- Export data to CSV format
- Generate plots of mouse movement over time
- Display movement magnitude histograms

## Recent Changes

### MouseMoveEvent Refactoring

The `MouseMoveEvent` struct has been refactored to use pcap timestamp fields (`ts_sec` and `ts_usec`) instead of a single f64 time field. This provides:
- More precise timestamp handling
- Direct compatibility with pcap data structures
- Helper methods for time conversion

### Offline LOD Module

A new hierarchical segmentation module has been added for efficient offline visualization:
- Build segment trees from captured events
- Adaptive rendering based on zoom/tolerance
- Cubic polynomial fits with SVD for numerical stability
- Cache-friendly design for performance

See [LOD Module Documentation](docs/LOD_MODULE.md) for detailed usage and integration guide.

## Requirements

- Windows OS (for USBPcap)
- [USBPcap](https://desowin.org/usbpcap/) installed at `C:\Program Files\USBPcap\`
- Rust toolchain

## Installation

```bash
cargo build --release
```

## Usage

### GUI Mode (Recommended)

Run the application with the GUI for interactive analysis:

```bash
# Start with GUI
MouseTesterRust.exe --gui -d 1.2.1

# Or use short flag
MouseTesterRust.exe -g -d 1.2.1
```

The GUI provides:
- **Deferred Plotting** - Plots are rendered only after capture is stopped for better performance
- **Real-time event counter** - See the number of events being captured live
- **Interactive plots** - Zoom, pan, and explore dx/dy movement over time with Level of Detail (LOD)
- **Histogram visualization** - Movement magnitude distribution
- **Events table** - Detailed view of individual events
- **Toggle controls** - Show/hide different visualization panels

Press **F2** to stop recording and display the analysis. On Windows, F2 works globally even when the GUI window is not focused. The plot will be drawn only after you press F2, which improves performance during capture.

### CLI Mode (Traditional)

Run without GUI for traditional command-line analysis:

```bash
MouseTesterRust.exe -d 1.2.1
```

This will:
1. Capture mouse events from the specified USB device
2. Press F2 to stop recording
3. Generate a CSV file (`output.csv`) with event data and statistics
4. Create a PNG plot (`mouse_plot.png`) and open it automatically

### Command Line Options

- `-d <bus.device.endpoint>` - Specify target USB device (e.g., `-d 1.2.1`)
  - `bus` - USB bus ID
  - `device` - Device address
  - `endpoint` - Endpoint number
- `--gui` or `-g` - Enable GUI mode (optional)

### Finding Your Mouse Device

Without specifying `-d`, the program will print all detected mouse events to help you identify your device's bus.device.endpoint values.

## Output

### GUI Mode
- Event counter updates in real-time during capture
- Statistics and plots are displayed after pressing F2 to stop capture
- Plots use Level of Detail (LOD) for better performance with large datasets

### CLI Mode
- `output.csv` - CSV file containing:
  - Individual events (dx, dy, time)
  - Summary statistics in comments at the end
- `mouse_plot.png` - High-resolution (3840x2160) plot showing dx (red) and -dy (blue) over time

## Statistics Provided

Both GUI and CLI modes calculate and display:
- Total event count
- Recording duration
- Total dx and dy displacement
- Total movement distance
- Average distance per event
- Events per second (polling rate)
- Average movement speed
- Movement magnitude histogram (distribution of movement sizes)

## Building from Source

```bash
# Debug build
cargo build

# Release build (recommended)
cargo build --release

# Check code without building
cargo check
```

## Dependencies

- `nalgebra` - Linear algebra for LOD polynomial fits
- `eframe` / `egui` - Modern GUI framework
- `egui_plot` - Interactive plotting for GUI
- `plotters` - Static plot generation for PNG output
- `pcap` / `pcap-parser` - USB packet capture parsing
- `anyhow` - Error handling
- `byteorder` - Binary data parsing

## Examples

Run the LOD module demonstration:

```bash
cargo run --example lod_demo
```

This example shows:
- Building hierarchical segment trees
- Collecting view data at different tolerances
- Performance metrics and data reduction

## Platform Notes

This tool is designed specifically for Windows with USBPcap installed. The GUI uses platform-agnostic rendering, but the USB capture functionality requires Windows and USBPcap.

## License

(Add your license information here)

## Contributing

(Add contribution guidelines here)
