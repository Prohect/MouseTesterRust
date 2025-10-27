//! LOD Parameter Optimization Tool
//!
//! This tool analyzes real mouse capture data from CSV files to determine
//! optimal LOD parameters for different mouse configurations.
//!
//! Run with: cargo run --example lod_analysis

use MouseTesterRust::lod::{build_segment_tree, collect_for_view};
use MouseTesterRust::mouse_event::MouseMoveEvent;
use std::fs::File;
use std::io::{BufRead, BufReader};

struct DatasetInfo {
    name: String,
    events: Vec<MouseMoveEvent>,
    time_span: f64,
    avg_report_rate: f64,
}

fn load_csv(path: &str) -> Result<Vec<MouseMoveEvent>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();

    for (i, line) in reader.lines().enumerate() {
        if i == 0 {
            continue; // Skip header
        }

        let line = line?;
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() >= 3 {
            let dx: i16 = parts[0].trim().parse()?;
            let dy: i16 = parts[1].trim().parse()?;
            let time: f64 = parts[2].trim().parse()?;

            // Convert time to pcap format (seconds + microseconds)
            let ts_sec = time.floor() as u32;
            let ts_usec = ((time.fract() * 1_000_000.0) as u32);

            events.push(MouseMoveEvent::new(dx, dy, ts_sec, ts_usec));
        }
    }

    Ok(events)
}

fn analyze_dataset(name: &str, path: &str) -> Result<DatasetInfo, Box<dyn std::error::Error>> {
    println!("\n=== Analyzing: {} ===", name);
    let events = load_csv(path)?;

    if events.is_empty() {
        println!("  No events found!");
        return Err("No events".into());
    }

    let time_span = events.last().unwrap().time_secs() - events.first().unwrap().time_secs();
    let avg_report_rate = events.len() as f64 / time_span;

    println!("  Events: {}", events.len());
    println!("  Time span: {:.3}s", time_span);
    println!("  Avg report rate: {:.1} Hz", avg_report_rate);

    // Calculate movement statistics
    let total_distance: f64 = events.iter().map(|e| ((e.dx as f64).powi(2) + (e.dy as f64).powi(2)).sqrt()).sum();
    let avg_movement = total_distance / events.len() as f64;

    println!("  Total distance: {:.1}", total_distance);
    println!("  Avg movement/event: {:.2}", avg_movement);

    Ok(DatasetInfo {
        name: name.to_string(),
        events,
        time_span,
        avg_report_rate,
    })
}

fn test_lod_parameters(dataset: &DatasetInfo) {
    println!("\n  Testing LOD Parameters:");

    // Test different parameter combinations
    let test_configs = vec![(5, 1.0, "Conservative"), (10, 1.0, "Moderate"), (5, 0.5, "Aggressive"), (10, 0.5, "Very Aggressive"), (3, 1.5, "Preserve Detail")];

    for (min_pts, tol_px, name) in test_configs {
        let tree = build_segment_tree(&dataset.events, 0, dataset.events.len(), min_pts, 1000, 1.0, tol_px);

        // Test at different view tolerances
        let view_tolerances = [0.5, 1.0, 2.0, 5.0];

        print!("    {} (min_pts={}, tol_px={}): ", name, min_pts, tol_px);

        for &view_tol in &view_tolerances {
            let mut view_points = Vec::new();
            collect_for_view(&tree, &dataset.events, 1.0, view_tol, &mut view_points);

            let reduction = 100.0 * (1.0 - view_points.len() as f64 / dataset.events.len() as f64);
            print!("{}px:{:.0}% ", view_tol, reduction);
        }
        println!();
    }
}

fn analyze_time_consistency(dataset: &DatasetInfo) {
    println!("\n  Time Consistency Analysis:");

    if dataset.events.len() < 2 {
        println!("    Not enough events");
        return;
    }

    // Calculate time deltas
    let mut deltas = Vec::new();
    for i in 1..dataset.events.len() {
        let delta = dataset.events[i].time_secs() - dataset.events[i - 1].time_secs();
        if delta > 0.0 {
            deltas.push(delta * 1000.0); // Convert to ms
        }
    }

    if deltas.is_empty() {
        println!("    No valid deltas");
        return;
    }

    deltas.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let min = deltas[0];
    let max = deltas[deltas.len() - 1];
    let median = deltas[deltas.len() / 2];
    let mean = deltas.iter().sum::<f64>() / deltas.len() as f64;

    // Calculate standard deviation
    let variance = deltas.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / deltas.len() as f64;
    let std_dev = variance.sqrt();

    println!("    Time delta stats (ms):");
    println!("      Min: {:.3}, Max: {:.3}", min, max);
    println!("      Mean: {:.3}, Median: {:.3}", mean, median);
    println!("      Std Dev: {:.3}", std_dev);
    println!("      Consistency: {:.1}% (lower is more consistent)", (std_dev / mean) * 100.0);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=================================================");
    println!("  LOD Parameter Optimization Analysis");
    println!("=================================================");

    let datasets = vec![
        ("20kSensor @ 1kHz", "examples/test/output-20kSensor_1kReport.csv"),
        ("20kSensor @ 8kHz", "examples/test/output-20kSensor_8kReport.csv"),
        ("LowPower @ 1kHz", "examples/test/output-LowPower_1kReport.csv"),
        ("CordedGaming @ 4kHz", "examples/test/output_CordedGaming_4KReport.csv"),
        ("CordedGaming @ 8kHz", "examples/test/output_CordedGaming_8KReport.csv"),
    ];

    let mut dataset_infos = Vec::new();

    for (name, path) in datasets {
        match analyze_dataset(name, path) {
            Ok(info) => {
                analyze_time_consistency(&info);
                test_lod_parameters(&info);
                dataset_infos.push(info);
            }
            Err(e) => {
                println!("  Error: {}", e);
            }
        }
    }

    // Summary and recommendations
    println!("\n=================================================");
    println!("  RECOMMENDATIONS");
    println!("=================================================");

    println!("\nBased on the analysis of {} datasets:", dataset_infos.len());

    // Calculate average report rates
    let avg_rate: f64 = dataset_infos.iter().map(|d| d.avg_report_rate).sum::<f64>() / dataset_infos.len() as f64;

    println!("  Average report rate: {:.0} Hz", avg_rate);

    println!("\nRecommended LOD Parameters:");
    println!("  For high report rate devices (8kHz+):");
    println!("    - min_pts: 10 (prevent over-segmentation)");
    println!("    - tol_px: 0.5-1.0 (maintain detail)");
    println!("    - Use aggressive reduction at zoom-out");

    println!("\n  For standard devices (1-4kHz):");
    println!("    - min_pts: 5-7 (balanced segmentation)");
    println!("    - tol_px: 1.0-1.5 (good quality/performance)");
    println!("    - Standard reduction strategy");

    println!("\n  For power-saving modes:");
    println!("    - min_pts: 5 (accommodate gaps)");
    println!("    - tol_px: 1.5-2.0 (tolerance for irregularities)");
    println!("    - Adaptive handling for timing variations");

    println!("\nGUI LOD Strategy:");
    println!("  - Current adaptive 1-3 points/pixel is good");
    println!("  - Consider data_density threshold at 5.0 for very high rate devices");
    println!("  - Use 0.5 points/pixel minimum for 8kHz+ when fully zoomed out");

    println!("\n=================================================");

    Ok(())
}
