# GUI Layout and Features

## Overview

The MouseTesterRust GUI provides an efficient interface for analyzing USB mouse events with deferred plotting for better performance. The GUI is built using egui/eframe and provides interactive plotting capabilities with Level of Detail (LOD) support.

## Performance Features

1. **Deferred Plotting**: Plots are only rendered after capture is stopped (press F2), eliminating real-time plotting overhead
2. **Level of Detail (LOD)**: When zoomed out or with many data points, the plot automatically downsamples to show fewer points for better rendering performance
3. **Efficient Repaints**: During capture, the GUI only repaints periodically to show event count, rather than continuously

## Window Layout

### During Capture
```
┌────────────────────────────────────────────────────────────────┐
│ 🖱 Mouse Event Analyzer │ Capturing: 1234 events              │
├──────────────┬─────────────────────────────────────────────────┤
│              │                                                  │
│  Controls    │           Main Content Area                     │
│              │                                                  │
│ ☑ Show Plot  │                                                  │
│ ☑ Statistics │      Capturing mouse events...                  │
│ ☑ Histogram  │      1234 events captured                       │
│ ☐ Events     │                                                  │
│   Table      │      Move your mouse to record events           │
│              │      Press F2 to stop capture and display       │
│ ● Recording  │      results                                    │
│ 1234 events  │                                                  │
│              │                                                  │
│ Press F2 to  │                                                  │
│ stop and     │                                                  │
│ plot         │                                                  │
└──────────────┴─────────────────────────────────────────────────┘
```

### After F2 (Stopped)
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
│ ● Stopped    │  │  Avg Distance/Event: 23.123               │  │
│ 1234 events  │  │  Events/sec: 227.675                      │  │
│              │  │  Avg Speed: 5251.942 units/s              │  │
│ Analysis     │  └──────────────────────────────────────────┘  │
│ complete     │                                                  │
│              │  ┌──────────────────────────────────────────┐  │
│              │  │   Movement Plot (dx and -dy vs time)     │  │
│              │  │                                           │  │
│              │  │   Interactive plot with:                 │  │
│              │  │   - Red line: dx values over time        │  │
│              │  │   - Blue line: -dy values over time      │  │
│              │  │   - Zoom/pan controls                    │  │
│              │  │   - Legend                               │  │
│              │  │   - LOD: Showing 800 of 1234 points      │  │
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
- **During Capture**: Shows "Capturing: X events"
- **After Capture**: Shows "Events: X" and "Duration: X.XXs"
- Updates periodically during capture, static after capture

### 2. Left Side Panel (Controls)
- **Toggle Switches**:
  - ☑ Show Plot - Toggle the movement plot display
  - ☑ Show Statistics - Toggle the statistics panel
  - ☑ Show Histogram - Toggle the histogram display
  - ☐ Show Events Table - Toggle the detailed events table
- **Status Indicator**:
  - **● Recording** (green) - Capture in progress
  - **● Stopped** (red) - Capture complete
- **Status Information**:
  - Current event count
  - Contextual instructions

### 3. Main Content Area (Scrollable)

#### Statistics Panel
Displays comprehensive metrics (only shown after capture is stopped):
- **Event Count**: Total number of mouse events captured
- **Duration**: Recording time in seconds (6 decimal places)
- **Total dx/dy**: Sum of all x and y movements
- **Total Distance**: Euclidean distance traveled (sum of movement magnitudes)
- **Avg Distance/Event**: Average movement per event
- **Events/sec**: Polling rate (events per second)
- **Avg Speed**: Average movement speed in units per second

Presented in a clean grid layout with labels and values

#### Movement Plot
Interactive plot showing mouse movement over time (only shown after capture is stopped):
- **X-axis**: Time in seconds
- **Y-axis**: Movement values (dx and -dy)
- **Red line**: dx values (horizontal movement)
- **Blue line**: -dy values (vertical movement, inverted for display)
- **Level of Detail (LOD)**: Automatically downsamples data when there are more points than screen pixels
  - Shows "Showing X of Y points (LOD applied)" when downsampling occurs
  - Target: ~2 points per horizontal pixel for smooth rendering
- **Interactive features**:
  - Zoom in/out with mouse wheel
  - Pan by dragging
  - Reset view by right-clicking
  - Legend showing line colors and labels
- View aspect ratio: 2.0 (wide format)
- **Performance**: Deferred rendering eliminates real-time plotting overhead

#### Movement Magnitude Histogram
Bar chart visualization (only shown after capture is stopped):
- **12 buckets**: Movement magnitudes divided into ranges
- **Green bars**: Count of events in each magnitude range
- **Labels**: Show range for each bucket [min-max)
- **Interactive**: Same zoom/pan features as movement plot
- Shows distribution of movement sizes

#### Events Table (Optional)
Detailed table view (only shown after capture is stopped):
- **Columns**: Index, dx, dy, Time (s)
- **Scrollable**: Shows last 100 events if more than 100 captured
- **Striped rows**: Alternating row colors for readability
- **Summary**: Shows "Showing last 100 of X events" when filtered

## Usage Flow

1. **Launch**: Start the application with `--gui` or `-g` flag
2. **Capturing**: GUI opens and immediately starts capturing events
   - Event counter updates periodically
   - Main area shows "Capturing mouse events..." message
   - No plots are rendered yet (deferred for performance)
3. **Stop Capture**: Press F2 to stop capture
   - Capture stops and takes snapshot of all events
   - Plot is rendered with LOD applied if needed
   - All statistics and visualizations become available
4. **Analysis**: 
   - Toggle panels on/off as needed
   - Zoom/pan plots to examine details
   - LOD automatically adjusts based on visible area
5. **Close**: Close the GUI window to exit

## Performance Characteristics

### Deferred Plotting
- **During Capture**: No plotting overhead, only periodic UI updates
- **After F2**: Plot rendered once with all data
- **Benefit**: Eliminates real-time plotting lag during high-frequency capture

### Level of Detail (LOD)
- **Adaptive Downsampling**: Reduces point count when zoomed out
- **Target Density**: ~2 points per horizontal pixel
- **Benefit**: Smooth rendering even with 10,000+ events
- **Example**: 5000 events on 1000px wide plot → ~2000 points displayed

## Technical Details

- **Framework**: egui/eframe (immediate mode GUI)
- **Plotting**: egui_plot for interactive charts
- **Thread-safe**: Uses Arc<Mutex<>> for shared event storage
- **Efficient updates**: Periodic repaints during capture, event-driven after stop
- **Deferred rendering**: Statistics and plots calculated only after F2 stop
- **LOD system**: Automatic downsampling based on screen width
- **Responsive**: Adjusts to window size changes

## Keyboard Shortcuts

- **F2**: Stop capture and display plot (works globally, even when GUI is not focused on Windows)

## Color Scheme

- **Background**: Default egui light/dark theme
- **dx line**: Red (#FF0000)
- **-dy line**: Blue (#0000FF)
- **Histogram bars**: Green (#64C864)
- **UI elements**: Standard egui theme colors
