# MouseTesterRust

A Rust-based tool for capturing and analyzing USB mouse events on Windows using USBPcap.

## Features

- Capture USB mouse movement data in real-time
- Analyze mouse movement patterns and statistics
- **NEW: Interactive GUI for easy visualization and analysis**
- Export data to CSV format
- Generate plots of mouse movement over time
- Display movement magnitude histograms

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
- **Real-time statistics** - Event count, duration, distance, speed
- **Interactive plots** - Zoom, pan, and explore dx/dy movement over time
- **Histogram visualization** - Movement magnitude distribution
- **Events table** - Detailed view of individual events
- **Toggle controls** - Show/hide different visualization panels

Press **F2** to stop recording and finalize the analysis.

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
- Real-time visualization in the application window
- All statistics and plots updated live as events are captured

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

- `eframe` / `egui` - Modern GUI framework
- `egui_plot` - Interactive plotting for GUI
- `plotters` - Static plot generation for PNG output
- `pcap` / `pcap-parser` - USB packet capture parsing
- `anyhow` - Error handling
- `byteorder` - Binary data parsing

## Platform Notes

This tool is designed specifically for Windows with USBPcap installed. The GUI uses platform-agnostic rendering, but the USB capture functionality requires Windows and USBPcap.

## License

(Add your license information here)

## Contributing

(Add contribution guidelines here)
