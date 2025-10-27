# MouseTesterRust - LOD Module Documentation

## Overview

The LOD (Level-of-Detail) module provides hierarchical segmentation for efficient offline visualization of mouse movement data. It uses cubic polynomial fits to create a tree structure that enables adaptive rendering at different zoom levels and tolerances.

## Key Features

- **Offline Processing**: Build segment trees after capture, not during live capture
- **Hierarchical Structure**: Recursive splitting based on error tolerance
- **Cubic Polynomial Fits**: Uses SVD for numerically stable least-squares fitting
- **Cache-Friendly**: Sequential traversal patterns for good performance
- **View-Adaptive**: Collect only the detail needed for current zoom/view
- **Read-Only After Build**: Thread-safe for concurrent rendering

## Architecture

### New MouseMoveEvent Structure

The refactored `MouseMoveEvent` uses separate timestamp fields for precision:

```rust
pub struct MouseMoveEvent {
    pub dx: i16,          // Horizontal movement
    pub dy: i16,          // Vertical movement
    pub ts_sec: u32,      // Timestamp seconds (from pcap)
    pub ts_usec: u32,     // Timestamp microseconds
}
```

Helper methods:
- `new(dx, dy, ts_sec, ts_usec)` - Create event with explicit timestamps
- `from_pcap_header(dx, dy, &rec)` - Create from pcap header
- `time_secs() -> f64` - Get time as floating-point seconds
- `time_micros() -> u64` - Get time as microseconds

### Parser Module

The parser module handles USB HID report parsing:

```rust
use MouseTesterRust::mouse_event::parser;

// Parse 8-byte report with Report ID
let event = parser::parse_with_report_id(payload, &pcap_header);

// Parse 7-byte report without Report ID
let event = parser::parse_without_report_id(payload, &pcap_header);

// Auto-detect format
let event = parser::parse_auto(payload, &pcap_header);
```

### LOD Module

The LOD module provides hierarchical segmentation:

```rust
use MouseTesterRust::lod::{build_segment_tree, collect_for_view};

// Build tree once after capture
let tree = build_segment_tree(
    &events,      // Event slice
    0,            // Start index
    events.len(), // End index
    5,            // min_pts: minimum points per segment
    1000,         // max_pts: maximum before splitting
    1.0,          // px_scale: pixel scale factor
    1.0           // tol_px: error tolerance in pixels
);

// Collect points for specific view (can be called many times)
let mut view_points = Vec::new();
collect_for_view(&tree, &events, 1.0, 0.5, &mut view_points);
// view_points now contains (time_micros, dx, dy) tuples
```

## Workflow Integration

### 1. Capture Phase (Runtime)

During USB capture, create events with timestamps:

```rust
// In capture loop
if let Some(event) = parser::parse_auto(payload, &rec_hdr) {
    events.push(event);
}
```

### 2. Build Phase (One-time, After Capture)

Build the LOD tree once capture is complete:

```rust
let tree = build_segment_tree(
    &events,
    0,
    events.len(),
    5,      // min_pts
    1000,   // max_pts
    1.0,    // px_scale
    1.0     // tol_px
);
```

### 3. View Phase (Interactive, Multiple Times)

Collect view data as user zooms/pans:

```rust
fn render_plot(tree: &SegmentNode, events: &[MouseMoveEvent], zoom: f64) {
    let view_tol = base_tolerance / zoom;  // Adjust for zoom level
    let mut points = Vec::new();
    collect_for_view(tree, events, 1.0, view_tol, &mut points);
    
    // Render only the collected points
    for (t_us, dx, dy) in points {
        plot_point(t_us as f64 / 1_000_000.0, dx, dy);
    }
}
```

## Parameter Guidelines

### min_pts (Minimum Points Per Segment)

- **Recommended**: 5-10
- **Purpose**: Prevents over-segmentation
- **Trade-off**: Lower = more detail, but deeper tree
- **Example**: 5 for general use, 10 for very large datasets

### max_pts (Maximum Points Before Splitting)

- **Recommended**: 500-1000
- **Purpose**: Balances tree depth vs. fit quality
- **Trade-off**: Higher = shallower tree but larger segments
- **Example**: 1000 for typical mouse data

### tol_px (Build Tolerance in Pixels)

- **Recommended**: 0.5-2.0
- **Purpose**: Error threshold for splitting during build
- **Trade-off**: Lower = more splits, higher detail
- **Example**: 1.0 for balanced quality/performance

### view_tol_px (View Tolerance in Pixels)

- **Recommended**: 0.5-2.0 (zoom-dependent)
- **Purpose**: Detail level for current view
- **Trade-off**: Lower = more points, higher fidelity
- **Example**: 
  - 2.0 for zoomed-out overview
  - 1.0 for normal view
  - 0.5 for zoomed-in inspection

### px_scale (Pixel Scale Factor)

- **Recommended**: 1.0
- **Purpose**: Convert movement units to pixel space
- **Adjust for**: DPI, display scale, coordinate system
- **Example**: 1.0 for direct units, 0.01 for counts/inch

## Performance Characteristics

### Time Complexity

- **Build**: O(n log n) where n = number of events
- **Collect**: O(n) worst case, typically O(m) where m << n
- **Memory**: O(n) for tree storage

### Memory Usage

The segment tree size is proportional to the event count:
- Each node stores: 2 Poly3 structs + metadata + children
- Typical overhead: ~2-3x the raw event data
- Consider: Keep tree in memory for repeated views

### Optimization Tips

1. **Build Once**: Tree building is expensive, reuse it
2. **Cache Views**: Cache collected points for common zoom levels
3. **Progressive Rendering**: Start with low detail, refine on demand
4. **Parallel Build**: Tree building is embarrassingly parallel (future work)

## Example Usage

See `examples/lod_demo.rs` for a complete demonstration:

```bash
cargo run --example lod_demo
```

The example shows:
- Creating synthetic mouse events
- Building a segment tree
- Collecting views at different tolerances
- Performance metrics and reduction ratios

## Use Cases

### Static Plot Export

Build tree once, export at multiple resolutions:

```rust
let tree = build_segment_tree(&events, 0, events.len(), 5, 1000, 1.0, 1.0);

// Export thumbnail (low detail)
let mut thumb_points = Vec::new();
collect_for_view(&tree, &events, 1.0, 5.0, &mut thumb_points);
export_png("thumbnail.png", &thumb_points);

// Export full resolution (high detail)
let mut full_points = Vec::new();
collect_for_view(&tree, &events, 1.0, 0.5, &mut full_points);
export_png("full_res.png", &full_points);
```

### Interactive Plotting

Adjust detail level based on zoom:

```rust
impl PlotApp {
    fn on_zoom_changed(&mut self, zoom: f64) {
        let tol = 1.0 / zoom.max(1.0);  // Higher zoom = lower tolerance
        self.view_points.clear();
        collect_for_view(&self.tree, &self.events, 1.0, tol, &mut self.view_points);
        self.redraw();
    }
}
```

### Memory-Constrained Rendering

Limit point count for low-memory devices:

```rust
fn collect_limited(tree: &SegmentNode, events: &[MouseMoveEvent], max_points: usize) -> Vec<(u64, f64, f64)> {
    let mut tol = 0.5;
    loop {
        let mut points = Vec::new();
        collect_for_view(tree, events, 1.0, tol, &mut points);
        if points.len() <= max_points {
            return points;
        }
        tol *= 1.5;  // Increase tolerance to reduce points
    }
}
```

## Testing

Run the test suite:

```bash
# Test library modules
cargo test --lib

# Test specific module
cargo test --lib mouse_event
cargo test --lib lod

# Run example
cargo run --example lod_demo
```

All tests include:
- Mouse event creation and parsing
- Timestamp conversion accuracy
- Polynomial fitting correctness
- Segment tree construction
- View collection with various tolerances

## Future Enhancements

Potential improvements for future versions:

1. **Parallel Tree Building**: Use rayon for concurrent segment processing
2. **Incremental Updates**: Add new events without full rebuild
3. **Spatial Queries**: Support region-based point collection
4. **Compression**: Store coefficients instead of raw points
5. **Adaptive Splitting**: Use R² or other metrics for split decisions
6. **GPU Acceleration**: Offload polynomial evaluation to GPU

## Troubleshooting

### "QR solve: unable to solve" or similar SVD errors

- Ensure data has at least 4 distinct points
- Check for NaN or infinite values in event data
- Verify timestamps are monotonically increasing

### Tree too deep / Out of memory

- Increase `min_pts` parameter (try 10-20)
- Decrease `max_pts` if using very large value
- Process data in chunks for extremely large captures

### Poor fit quality / Too many points

- Decrease `tol_px` during build for better fits
- Increase `view_tol_px` when collecting for less detail
- Adjust `px_scale` based on your coordinate system

### Slow rendering after zoom

- Cache collected points for common zoom levels
- Use progressive rendering (start coarse, refine)
- Consider spatial indexing for large datasets

## License

This module follows the same license as MouseTesterRust.

## References

- USB HID specification for mouse report formats
- Numerical recipes for polynomial fitting algorithms
- SVD-based least-squares for overdetermined systems
- Hierarchical data structures for LOD rendering
