//! Example demonstrating the use of the offline LOD module
//!
//! This example creates synthetic mouse movement events and demonstrates:
//! 1. Building a hierarchical segment tree from the events
//! 2. Collecting view data at different tolerance levels
//! 3. Showing how the LOD module reduces data points for efficient rendering
//!
//! Run with: cargo run --example lod_demo

use MouseTesterRust::lod::{build_segment_tree, collect_for_view};
use MouseTesterRust::mouse_event::MouseMoveEvent;

fn main() {
    println!("=== Offline LOD Module Demo ===\n");

    // Create synthetic mouse movement data
    // Simulate a curved mouse path with some noise
    let mut events = Vec::new();
    let num_events = 200;

    for i in 0..num_events {
        let t = i as f64 / 10.0; // Time in seconds (0.1s intervals)
        let ts_sec = t.floor() as u32;
        let ts_usec = ((t.fract() * 1_000_000.0) as u32);

        // Create a sinusoidal pattern for dx and dy
        let dx = (50.0 * (t * 0.5).sin()) as i16;
        let dy = (30.0 * (t * 0.3).cos()) as i16;

        events.push(MouseMoveEvent::new(dx, dy, ts_sec, ts_usec));
    }

    println!("Created {} synthetic mouse events", events.len());
    println!("Time span: {:.2} seconds\n", events.last().unwrap().time_secs());

    // Build segment tree with recommended parameters
    let min_pts = 5; // Minimum points per segment
    let max_pts = 1000; // Maximum points before splitting
    let px_scale = 1.0; // Pixel scale factor
    let tol_px = 1.0; // Error tolerance in pixels

    println!("Building segment tree...");
    println!("  min_pts: {}", min_pts);
    println!("  max_pts: {}", max_pts);
    println!("  px_scale: {}", px_scale);
    println!("  tol_px: {}\n", tol_px);

    let tree = build_segment_tree(&events, 0, events.len(), min_pts, max_pts, px_scale, tol_px);

    println!("Segment tree built successfully!");
    println!("  Root segment: [{}, {})", tree.start, tree.end);
    println!("  Root RMSE: {:.3} pixels", tree.rmse_px);
    println!("  Number of children: {}\n", tree.children.len());

    // Collect data for different view tolerances
    let tolerances = [0.5, 1.0, 2.0, 5.0];

    println!("Collecting view data at different tolerances:");
    println!("(tolerance → points collected → reduction)");
    println!("{}", "-".repeat(50));

    for &view_tol in &tolerances {
        let mut view_points = Vec::new();
        collect_for_view(&tree, &events, px_scale, view_tol, &mut view_points);

        let reduction = 100.0 * (1.0 - view_points.len() as f64 / events.len() as f64);

        println!("  tol={:.1}px → {:3} points ({:5.1}% reduction)", view_tol, view_points.len(), reduction);
    }

    println!("\n=== Use Case Scenarios ===\n");

    println!("1. Zoomed Out View (low detail needed):");
    let mut coarse_view = Vec::new();
    collect_for_view(&tree, &events, px_scale, 5.0, &mut coarse_view);
    println!("   Using tol=5.0px → {} points (from {} original)", coarse_view.len(), events.len());
    println!("   Suitable for: overview, thumbnails, minimap\n");

    println!("2. Normal View (balanced detail):");
    let mut normal_view = Vec::new();
    collect_for_view(&tree, &events, px_scale, 1.0, &mut normal_view);
    println!("   Using tol=1.0px → {} points (from {} original)", normal_view.len(), events.len());
    println!("   Suitable for: standard plotting, general visualization\n");

    println!("3. Zoomed In View (high detail needed):");
    let mut fine_view = Vec::new();
    collect_for_view(&tree, &events, px_scale, 0.5, &mut fine_view);
    println!("   Using tol=0.5px → {} points (from {} original)", fine_view.len(), events.len());
    println!("   Suitable for: close inspection, analysis\n");

    println!("=== Integration Workflow ===\n");
    println!("1. Capture Phase:");
    println!("   - Record mouse events during USB capture");
    println!("   - Store full event stream in memory\n");

    println!("2. Build Phase (one-time, after capture):");
    println!("   - Call build_segment_tree() on captured events");
    println!("   - Store tree for reuse with different views\n");

    println!("3. View Phase (multiple times, interactive):");
    println!("   - User zooms/pans in plotting interface");
    println!("   - Call collect_for_view() with appropriate tolerance");
    println!("   - Render only the returned points\n");

    println!("=== Performance Notes ===\n");
    println!("- Tree building is O(n log n), done once after capture");
    println!("- View collection is O(n), but typically returns << n points");
    println!("- Memory: tree size is proportional to event count");
    println!("- Cache-friendly: tree traversal is sequential");
    println!("- Thread-safe: read-only after building\n");

    println!("=== Recommended Parameters ===\n");
    println!("min_pts:  5-10   (prevents over-segmentation)");
    println!("max_pts:  500-1000 (balances tree depth vs. fit quality)");
    println!("tol_px:   0.5-2.0  (view-dependent, lower = more detail)");
    println!("px_scale: 1.0      (adjust based on DPI/zoom level)\n");

    // Sample some points to show the data format
    println!("=== Sample Output Data ===\n");
    let mut sample_view = Vec::new();
    collect_for_view(&tree, &events, px_scale, 1.0, &mut sample_view);

    println!("First 5 points (time_micros, dx, dy):");
    for (i, &(t_us, dx, dy)) in sample_view.iter().take(5).enumerate() {
        println!("  {:2}: t={:10}µs  dx={:6.1}  dy={:6.1}", i, t_us, dx, dy);
    }

    println!("\nDemo complete!");
}
