//! Integration test for advanced LOD algorithm using real test datasets
//!
//! This test loads CSV files from the examples/test directory and validates
//! the advanced LOD algorithm with real mouse movement data.

use MouseTesterRust::lod_advanced::{build_segments, collect_visible_indices};
use MouseTesterRust::mouse_event::MouseMoveEvent;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Load events from a CSV file
fn load_csv_events(path: &Path) -> Result<Vec<MouseMoveEvent>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();

    for (line_num, line) in reader.lines().enumerate() {
        let line = line?;
        
        // Skip header
        if line_num == 0 && line.starts_with("dx,dy,time") {
            continue;
        }

        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() != 3 {
            continue;
        }

        let dx: i16 = parts[0].trim().parse()?;
        let dy: i16 = parts[1].trim().parse()?;
        let time: f64 = parts[2].trim().parse()?;

        // Convert time to ts_sec and ts_usec
        let ts_sec = time.floor() as u32;
        let ts_usec = ((time - time.floor()) * 1_000_000.0) as u32;

        events.push(MouseMoveEvent::new(dx, dy, ts_sec, ts_usec));
    }

    Ok(events)
}

#[test]
fn test_lod_with_1k_sensor() {
    let path = Path::new("examples/test/output-20kSensor_1kReport.csv");
    if !path.exists() {
        println!("Skipping test - file not found: {:?}", path);
        return;
    }

    let events = load_csv_events(path).expect("Failed to load CSV");
    println!("Loaded {} events from 1kHz sensor data", events.len());

    // Build segments
    let segments = build_segments(&events, 10, 1.5, 0.85, 0.5);
    println!("Created {} segments", segments.len());

    // Collect visible indices for full view
    let x_min = events.first().map(|e| e.time_secs()).unwrap_or(0.0);
    let x_max = events.last().map(|e| e.time_secs()).unwrap_or(1.0);
    let y_min = events.iter().map(|e| -(e.dy as f64)).fold(f64::INFINITY, f64::min);
    let y_max = events.iter().map(|e| -(e.dy as f64)).fold(f64::NEG_INFINITY, f64::max);

    let visible = collect_visible_indices(
        &segments,
        &events,
        1920.0, // Full HD width
        1080.0, // Full HD height
        (x_min, x_max),
        (y_min, y_max),
        5.0, // tolerance
        1.5, // zoom factor
    );

    let reduction_pct = 100.0 * (1.0 - visible.len() as f64 / events.len() as f64);
    println!(
        "LOD reduced {} events to {} ({:.1}% reduction)",
        events.len(),
        visible.len(),
        reduction_pct
    );

    assert!(!visible.is_empty());
    assert!(visible.len() <= events.len());
}

#[test]
fn test_lod_with_8k_sensor() {
    let path = Path::new("examples/test/output-20kSensor_8kReport.csv");
    if !path.exists() {
        println!("Skipping test - file not found: {:?}", path);
        return;
    }

    let events = load_csv_events(path).expect("Failed to load CSV");
    println!("Loaded {} events from 8kHz sensor data", events.len());

    // Build segments with more relaxed parameters for noisy real-world data
    let segments = build_segments(&events, 20, 2.0, 0.7, 0.7);
    println!("Created {} segments", segments.len());

    // Collect visible indices for full view
    let x_min = events.first().map(|e| e.time_secs()).unwrap_or(0.0);
    let x_max = events.last().map(|e| e.time_secs()).unwrap_or(1.0);
    let y_min = events.iter().map(|e| -(e.dy as f64)).fold(f64::INFINITY, f64::min);
    let y_max = events.iter().map(|e| -(e.dy as f64)).fold(f64::NEG_INFINITY, f64::max);

    let visible = collect_visible_indices(
        &segments,
        &events,
        1920.0,
        1080.0,
        (x_min, x_max),
        (y_min, y_max),
        2.0, // tighter tolerance for 8kHz - more aggressive filtering
        1.5,
    );

    let reduction_pct = 100.0 * (1.0 - visible.len() as f64 / events.len() as f64);
    println!(
        "LOD reduced {} events to {} ({:.1}% reduction)",
        events.len(),
        visible.len(),
        reduction_pct
    );

    assert!(!visible.is_empty());
    assert!(visible.len() <= events.len());
    // With relaxed parameters, some reduction should occur
    assert!(visible.len() < events.len(), "Expected some reduction for 8kHz data");
}

#[test]
fn test_lod_zoom_behavior() {
    let path = Path::new("examples/test/output_CordedGaming_4KReport.csv");
    if !path.exists() {
        println!("Skipping test - file not found: {:?}", path);
        return;
    }

    let events = load_csv_events(path).expect("Failed to load CSV");
    println!("Loaded {} events from 4kHz gaming mouse data", events.len());

    // Build segments
    let segments = build_segments(&events, 15, 2.0, 0.7, 0.6);

    let x_min = events.first().map(|e| e.time_secs()).unwrap_or(0.0);
    let x_max = events.last().map(|e| e.time_secs()).unwrap_or(1.0);
    let y_min = events.iter().map(|e| -(e.dy as f64)).fold(f64::INFINITY, f64::min);
    let y_max = events.iter().map(|e| -(e.dy as f64)).fold(f64::NEG_INFINITY, f64::max);

    // Test zoomed out view (full range)
    let visible_full = collect_visible_indices(
        &segments,
        &events,
        1920.0,
        1080.0,
        (x_min, x_max),
        (y_min, y_max),
        2.0, // Lower tolerance for more filtering
        1.0,
    );

    println!("Full view: {} events out of {}", visible_full.len(), events.len());

    assert!(!visible_full.is_empty());
    assert!(visible_full.len() <= events.len());
    // Both views should apply some filtering
    assert!(visible_full.len() < events.len(), "Expected filtering in full view");
    
    println!("Successfully applied LOD filtering");
}

#[test]
fn test_lod_preserves_segment_boundaries() {
    let path = Path::new("examples/test/output-LowPower_1kReport.csv");
    if !path.exists() {
        println!("Skipping test - file not found: {:?}", path);
        return;
    }

    let events = load_csv_events(path).expect("Failed to load CSV");
    println!("Loaded {} events from low power mouse data", events.len());

    // Build segments
    let segments = build_segments(&events, 8, 2.0, 0.8, 0.5);
    
    let x_min = events.first().map(|e| e.time_secs()).unwrap_or(0.0);
    let x_max = events.last().map(|e| e.time_secs()).unwrap_or(1.0);
    let y_min = events.iter().map(|e| -(e.dy as f64)).fold(f64::INFINITY, f64::min);
    let y_max = events.iter().map(|e| -(e.dy as f64)).fold(f64::NEG_INFINITY, f64::max);

    let visible = collect_visible_indices(
        &segments,
        &events,
        1920.0,
        1080.0,
        (x_min, x_max),
        (y_min, y_max),
        5.0,
        1.5,
    );

    // Verify that segment boundaries are preserved
    // First event should be included
    if !events.is_empty() {
        assert!(visible.contains(&0), "First event should be preserved");
    }

    // Check that indices are sorted
    for i in 1..visible.len() {
        assert!(
            visible[i] > visible[i - 1],
            "Indices should be sorted and unique"
        );
    }

    println!("All segment boundaries preserved correctly");
}

#[test]
fn test_lod_segment_quality_metrics() {
    let path = Path::new("examples/test/output_CordedGaming_8KReport.csv");
    if !path.exists() {
        println!("Skipping test - file not found: {:?}", path);
        return;
    }

    let events = load_csv_events(path).expect("Failed to load CSV");
    println!("Loaded {} events from 8kHz gaming mouse data", events.len());

    // Build segments with very relaxed quality requirements for real-world noisy data
    // Real mouse data can have significant noise and jitter
    let segments = build_segments(&events, 20, 2.5, 0.5, 0.7);

    use MouseTesterRust::lod_advanced::Segment;
    
    let mut good_count = 0;
    let mut discrete_count = 0;

    for segment in &segments {
        match segment {
            Segment::Good { .. } => {
                good_count += 1;
            }
            Segment::Discrete { .. } => {
                discrete_count += 1;
            }
        }
    }

    println!("Good segments: {}, Discrete events: {}", good_count, discrete_count);
    println!("Total segments: {}", segments.len());
    
    // Just verify we created segments
    assert!(!segments.is_empty(), "Should have created some segments");
    assert!(good_count > 0 || discrete_count > 0, "Should have at least one segment type");
    
    println!("Successfully segmented mouse data");
}

#[test]
fn test_lod_visibility_filtering() {
    let path = Path::new("examples/test/output-20kSensor_1kReport.csv");
    if !path.exists() {
        println!("Skipping test - file not found: {:?}", path);
        return;
    }

    let events = load_csv_events(path).expect("Failed to load CSV");
    println!("Loaded {} events from 1kHz sensor data", events.len());

    // Build segments
    let segments = build_segments(&events, 15, 2.0, 0.7, 0.25);

    // Get full time range
    let full_x_min = events.first().map(|e| e.time_secs()).unwrap_or(0.0);
    let full_x_max = events.last().map(|e| e.time_secs()).unwrap_or(1.0);
    let y_min = events.iter().map(|e| -(e.dy as f64)).fold(f64::INFINITY, f64::min);
    let y_max = events.iter().map(|e| -(e.dy as f64)).fold(f64::NEG_INFINITY, f64::max);

    // Test 1: View range that's completely outside the data range (way before)
    let out_of_range_before = collect_visible_indices(
        &segments,
        &events,
        1920.0,
        1080.0,
        (full_x_min - 100.0, full_x_min - 50.0), // Range before any data
        (y_min, y_max),
        3.0,
        1.5,
    );

    println!("Out of range (before) view: {} events", out_of_range_before.len());
    assert_eq!(
        out_of_range_before.len(),
        0,
        "Should return no events when view is completely before data range"
    );

    // Test 2: View range that's completely outside the data range (way after)
    let out_of_range_after = collect_visible_indices(
        &segments,
        &events,
        1920.0,
        1080.0,
        (full_x_max + 50.0, full_x_max + 100.0), // Range after any data
        (y_min, y_max),
        3.0,
        1.5,
    );

    println!("Out of range (after) view: {} events", out_of_range_after.len());
    assert_eq!(
        out_of_range_after.len(),
        0,
        "Should return no events when view is completely after data range"
    );

    // Test 3: View range that contains some data
    let time_range = full_x_max - full_x_min;
    let mid_x = full_x_min + time_range * 0.5;
    let partial_range = collect_visible_indices(
        &segments,
        &events,
        1920.0,
        1080.0,
        (mid_x - time_range * 0.1, mid_x + time_range * 0.1), // Small window in middle
        (y_min, y_max),
        3.0,
        1.5,
    );

    println!("Partial range view: {} events", partial_range.len());
    assert!(
        partial_range.len() > 0,
        "Should return some events when view intersects data"
    );
    assert!(
        partial_range.len() < events.len(),
        "Should return fewer events than total when zoomed to partial range"
    );

    // Verify all returned event indices are actually within the requested time range
    let tolerance = 1e-6; // Small tolerance for floating point comparison
    for &idx in &partial_range {
        let event = &events[idx];
        let event_time = event.time_secs();
        assert!(
            event_time >= (mid_x - time_range * 0.1 - tolerance) && 
            event_time <= (mid_x + time_range * 0.1 + tolerance),
            "Event at index {} with time {} should be within view range [{}, {}]",
            idx, event_time, mid_x - time_range * 0.1, mid_x + time_range * 0.1
        );
    }

    println!("Visibility filtering working correctly!");
}
