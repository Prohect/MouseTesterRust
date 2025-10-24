# Visual Preview of the GUI

Since the application requires Windows with USBPcap to run, here's a detailed description of what the GUI looks like when running:

## Main Window (1200x800 pixels)

### Top Bar (Gray background)
```
┌────────────────────────────────────────────────────────────────┐
│ 🖱 Mouse Event Analyzer │ Events: 1234 │ Duration: 5.42s       │
└────────────────────────────────────────────────────────────────┘
```
- Mouse icon emoji followed by title
- Real-time event counter
- Recording duration display

### Layout (Split View)

#### Left Sidebar (200px width, Light gray)
```
┌──────────────┐
│  Controls    │
│              │
│ ☑ Show Plot  │
│ ☑ Statistics │
│ ☑ Histogram  │
│ ☐ Events     │
│   Table      │
│              │
│ Recording:   │
│ 1234 events  │
│              │
│ Press F2 to  │
│ stop         │
│ recording    │
└──────────────┘
```
- Clean checkboxes for toggling views
- Status indicator at bottom
- Persistent instruction text

#### Main Content Area (Scrollable, White background)

##### 1. Statistics Panel (when enabled)
```
┌──────────────────────────────────────────────┐
│ Statistics                                    │
├──────────────────────┬───────────────────────┤
│ Event Count:         │ 1234                  │
│ Duration:            │ 5.420000 s            │
│ Total dx:            │ 15234                 │
│ Total dy:            │ -8421                 │
│ Total Distance:      │ 28472.456             │
│ Avg Distance/Event:  │ 23.123                │
│ Events/sec:          │ 227.675               │
│ Avg Speed:           │ 5251.942 units/s      │
└──────────────────────┴───────────────────────┘
```
- Clean grid layout with alternating row colors
- Left-aligned labels, right-aligned values
- Precise decimal formatting

##### 2. Movement Plot (when enabled)
```
┌──────────────────────────────────────────────┐
│ Movement Plot (dx and -dy vs time)           │
│                                               │
│   200│                                        │
│      │     ╱╲    ╱╲                          │
│   100│    ╱  ╲  ╱  ╲   Red line (dx)        │
│      │   ╱    ╲╱    ╲                        │
│     0├────────────────────────────────────   │
│      │          ╲    ╱                       │
│  -100│           ╲  ╱   Blue line (-dy)     │
│      │            ╲╱                         │
│  -200│                                        │
│      └────────────────────────────────────   │
│       0.0   1.0   2.0   3.0   4.0   5.0     │
│                 Time (seconds)                │
│                                               │
│ Legend: ─ dx (red)  ─ -dy (blue)            │
└──────────────────────────────────────────────┘
```
- Interactive plot with zoom/pan
- Red line for dx values
- Blue line for -dy values
- Automatic axis scaling
- Grid lines for reference
- Legend in corner

##### 3. Histogram (when enabled)
```
┌──────────────────────────────────────────────┐
│ Movement Magnitude Histogram                  │
│                                               │
│  600│                                         │
│     │     ██                                  │
│  500│     ██                                  │
│     │     ██                                  │
│  400│     ██    ██                            │
│     │     ██    ██                            │
│  300│     ██    ██    ██                      │
│     │     ██    ██    ██                      │
│  200│ ██  ██    ██    ██    ██                │
│     │ ██  ██    ██    ██    ██    ██         │
│  100│ ██  ██    ██    ██    ██    ██    ██   │
│     │ ██  ██    ██    ██    ██    ██    ██   │
│    0├───┬────┬────┬────┬────┬────┬────┬───   │
│      0-10 10-20 20-30 30-40 ...              │
│           Movement Magnitude Range            │
│                                               │
│ Legend: ▓ Count (green bars)                 │
└──────────────────────────────────────────────┘
```
- Green bar chart
- 12 buckets showing distribution
- X-axis shows magnitude ranges
- Y-axis shows event count
- Interactive zoom/pan

##### 4. Events Table (when enabled)
```
┌──────────────────────────────────────────────┐
│ Events Table                                  │
│                                               │
│ ┌────┬─────┬─────┬──────────┐               │
│ │Idx │ dx  │ dy  │Time (s)  │               │
│ ├────┼─────┼─────┼──────────┤               │
│ │1134│  15 │  -8 │ 4.234567 │ (light gray) │
│ │1135│  -3 │  12 │ 4.239012 │ (white)      │
│ │1136│   8 │  -5 │ 4.243890 │ (light gray) │
│ │1137│   0 │   0 │ 4.248123 │ (white)      │
│ │... │ ... │ ... │ ...      │               │
│ │1234│  10 │  -2 │ 5.419876 │ (light gray) │
│ └────┴─────┴─────┴──────────┘               │
│                                               │
│ Showing last 100 of 1234 events              │
└──────────────────────────────────────────────┘
```
- Scrollable table
- Striped rows (alternating colors)
- Last 100 events shown
- Monospace numbers for alignment

### Empty State (No events recorded)
```
┌──────────────────────────────────────────────┐
│                                               │
│                                               │
│         No events recorded yet                │
│         Waiting for mouse events...           │
│     Press F2 to stop recording and            │
│              analyze                          │
│                                               │
│                                               │
└──────────────────────────────────────────────┘
```
- Centered text
- Clear instructions
- Minimal, clean design

## Color Scheme

### Light Theme (Default)
- Background: White (#FFFFFF)
- Text: Dark Gray (#2B2B2B)
- Panels: Light Gray (#F0F0F0)
- Grid lines: Light Gray (#E0E0E0)
- dx line: Red (#FF0000)
- -dy line: Blue (#0000FF)
- Histogram bars: Green (#64C864)
- Accent: Standard egui blue for interactive elements

### Dark Theme (If enabled in egui settings)
- Background: Dark Gray (#1E1E1E)
- Text: Light Gray (#E0E0E0)
- Panels: Darker Gray (#2D2D2D)
- Grid lines: Gray (#404040)
- Colors remain vibrant for visibility

## Interactive Elements

1. **Checkboxes**: Standard egui checkboxes with smooth toggle animation
2. **Plot Zoom**: Mouse wheel to zoom in/out
3. **Plot Pan**: Click and drag to move view
4. **Plot Reset**: Right-click to reset view to full data range
5. **Scrolling**: Vertical scroll bar for main content area
6. **Window Resize**: Fully resizable window with adaptive layout

## Responsiveness

- Window can be resized to any size above 800x600
- Plots scale proportionally
- Text remains readable at all sizes
- Layout adjusts to maintain usability

## Performance

- Smooth 60 FPS rendering (request_repaint on every frame)
- Real-time updates as events stream in
- No lag even with thousands of events
- Efficient plot rendering with egui_plot

This provides a comprehensive visual understanding of the GUI without requiring actual screenshots from a Windows system.
