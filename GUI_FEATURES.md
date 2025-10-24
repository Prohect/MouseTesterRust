# GUI Layout and Features

## Overview

The MouseTesterRust GUI provides a comprehensive, real-time interface for analyzing USB mouse events. The GUI is built using egui/eframe and provides interactive plotting capabilities.

## Window Layout

```
┌────────────────────────────────────────────────────────────────┐
│ 🖱 Mouse Event Analyzer │ Events: 1234 │ Duration: 5.42s     │
├──────────────┬─────────────────────────────────────────────────┤
│              │                                                  │
│  Controls    │           Main Content Area                     │
│              │                                                  │
│ ☑ Show Plot  │  ┌──────────────────────────────────────────┐  │
│ ☑ Statistics │  │         Statistics                        │  │
│ ☑ Histogram  │  │  Event Count: 1234                        │  │
│ ☐ Events     │  │  Duration: 5.420000 s                     │  │
│   Table      │  │  Total dx: 15234                          │  │
│              │  │  Total dy: -8421                          │  │
│              │  │  Total Distance: 28472.456                │  │
│ Recording:   │  │  Avg Distance/Event: 23.123               │  │
│ 1234 events  │  │  Events/sec: 227.675                      │  │
│              │  │  Avg Speed: 5251.942 units/s              │  │
│ Press F2 to  │  └──────────────────────────────────────────┘  │
│ stop         │                                                  │
│              │  ┌──────────────────────────────────────────┐  │
│              │  │   Movement Plot (dx and -dy vs time)     │  │
│              │  │                                           │  │
│              │  │   Interactive plot with:                 │  │
│              │  │   - Red line: dx values over time        │  │
│              │  │   - Blue line: -dy values over time      │  │
│              │  │   - Zoom/pan controls                    │  │
│              │  │   - Legend                               │  │
│              │  │                                           │  │
│              │  └──────────────────────────────────────────┘  │
│              │                                                  │
│              │  ┌──────────────────────────────────────────┐  │
│              │  │  Movement Magnitude Histogram             │  │
│              │  │                                           │  │
│              │  │   Bar chart showing distribution of      │  │
│              │  │   movement magnitudes in 12 buckets      │  │
│              │  │                                           │  │
│              │  └──────────────────────────────────────────┘  │
│              │                                                  │
└──────────────┴─────────────────────────────────────────────────┘
```

## Features in Detail

### 1. Top Panel (Header)
- **Title**: "🖱 Mouse Event Analyzer" with mouse icon
- **Event Counter**: Real-time count of captured events
- **Duration**: Total recording time in seconds
- Updates continuously as events are captured

### 2. Left Side Panel (Controls)
- **Toggle Switches**:
  - ☑ Show Plot - Toggle the movement plot display
  - ☑ Show Statistics - Toggle the statistics panel
  - ☑ Show Histogram - Toggle the histogram display
  - ☐ Show Events Table - Toggle the detailed events table
- **Status Display**:
  - Current event count
  - Instruction to press F2 to stop

### 3. Main Content Area (Scrollable)

#### Statistics Panel
Displays comprehensive metrics:
- **Event Count**: Total number of mouse events captured
- **Duration**: Recording time in seconds (6 decimal places)
- **Total dx/dy**: Sum of all x and y movements
- **Total Distance**: Euclidean distance traveled (sum of movement magnitudes)
- **Avg Distance/Event**: Average movement per event
- **Events/sec**: Polling rate (events per second)
- **Avg Speed**: Average movement speed in units per second

Presented in a clean grid layout with labels and values

#### Movement Plot
Interactive plot showing mouse movement over time:
- **X-axis**: Time in seconds
- **Y-axis**: Movement values (dx and -dy)
- **Red line**: dx values (horizontal movement)
- **Blue line**: -dy values (vertical movement, inverted for display)
- **Interactive features**:
  - Zoom in/out with mouse wheel
  - Pan by dragging
  - Reset view by right-clicking
  - Legend showing line colors and labels
- View aspect ratio: 2.0 (wide format)

#### Movement Magnitude Histogram
Bar chart visualization:
- **12 buckets**: Movement magnitudes divided into ranges
- **Green bars**: Count of events in each magnitude range
- **Labels**: Show range for each bucket [min-max)
- **Interactive**: Same zoom/pan features as movement plot
- Shows distribution of movement sizes

#### Events Table (Optional)
Detailed table view:
- **Columns**: Index, dx, dy, Time (s)
- **Scrollable**: Shows last 100 events if more than 100 captured
- **Striped rows**: Alternating row colors for readability
- **Summary**: Shows "Showing last 100 of X events" when filtered

## Empty State

When no events have been captured yet:
```
┌─────────────────────────────────────┐
│                                     │
│     No events recorded yet          │
│     Waiting for mouse events...     │
│  Press F2 to stop recording and     │
│          analyze                    │
│                                     │
└─────────────────────────────────────┘
```

## Usage Flow

1. **Launch**: Start the application with `--gui` or `-g` flag
2. **Recording**: GUI opens and immediately starts capturing events
   - Events are displayed in real-time
   - All visualizations update continuously
3. **Interaction**: 
   - Toggle panels on/off as needed
   - Zoom/pan plots to examine details
   - Scroll to see all visualizations
4. **Stop**: Press F2 to stop recording
   - GUI remains open showing final results
   - Can still interact with plots and data
5. **Close**: Close the GUI window to exit

## Technical Details

- **Framework**: egui/eframe (immediate mode GUI)
- **Plotting**: egui_plot for interactive charts
- **Thread-safe**: Uses Arc<Mutex<>> for shared event storage
- **Real-time updates**: GUI updates continuously with request_repaint()
- **Responsive**: Adjusts to window size changes

## Keyboard Shortcuts

- **F2**: Stop recording (works globally, even when GUI is not focused on Windows)

## Color Scheme

- **Background**: Default egui light/dark theme
- **dx line**: Red (#FF0000)
- **-dy line**: Blue (#0000FF)
- **Histogram bars**: Green (#64C864)
- **UI elements**: Standard egui theme colors
