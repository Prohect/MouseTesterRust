use crate::MouseMoveEvent;
use eframe::egui;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;

#[cfg(windows)]
use crate::key_utils;

pub struct MouseAnalyzerGui {
    events: Arc<Mutex<Vec<MouseMoveEvent>>>,
    stop_flag: Arc<AtomicBool>,
    show_plot: bool,
    show_stats: bool,
    show_histogram: bool,
    show_events_table: bool,
    is_capturing: bool,
    captured_events: Vec<MouseMoveEvent>,       // Events snapshot when capture stopped
    last_f2_state: bool,                        // For edge detection
    target_device: Option<crate::TargetDevice>, // Store target device for restarts
    // LOD state for intelligent updates (indices-based)
    cached_lod_indices: Vec<usize>,
    lod_pyramid: Vec<Vec<usize>>, // Lazy LOD pyramid for fast lookups
    last_plot_bounds: Option<PlotBounds>,
    last_target_points: Option<usize>, // Track target_points for coarsen-from-previous
    last_events_len: usize,            // For cache invalidation on capture reset
    lod_threshold: f64,                // Threshold for triggering LOD recalculation (0.1 = 10% change)
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PlotBounds {
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
}

impl MouseAnalyzerGui {
    pub fn new(events: Arc<Mutex<Vec<MouseMoveEvent>>>, stop_flag: Arc<AtomicBool>, target_device: Option<crate::TargetDevice>) -> Self {
        Self {
            events,
            stop_flag,
            show_plot: true,
            show_stats: true,
            show_histogram: true,
            show_events_table: false,
            is_capturing: true, // Start capturing initially
            captured_events: Vec::new(),
            last_f2_state: false,
            target_device,
            cached_lod_indices: Vec::new(),
            lod_pyramid: Vec::new(),
            last_plot_bounds: None,
            last_target_points: None,
            last_events_len: 0,
            lod_threshold: 0.1, // 10% change threshold
        }
    }

    /// Check if plot bounds have changed significantly
    fn bounds_changed_significantly(&self, new_bounds: &PlotBounds) -> bool {
        match self.last_plot_bounds {
            None => true, // First time, always recalculate
            Some(old_bounds) => {
                let x_range_old = (old_bounds.x_max - old_bounds.x_min).abs();
                let y_range_old = (old_bounds.y_max - old_bounds.y_min).abs();
                let x_range_new = (new_bounds.x_max - new_bounds.x_min).abs();
                let y_range_new = (new_bounds.y_max - new_bounds.y_min).abs();

                // Check if range changed by more than threshold
                let x_change = ((x_range_new - x_range_old) / x_range_old.max(1e-6)).abs();
                let y_change = ((y_range_new - y_range_old) / y_range_old.max(1e-6)).abs();

                // Also check if center position moved significantly
                let x_center_old = (old_bounds.x_min + old_bounds.x_max) / 2.0;
                let x_center_new = (new_bounds.x_min + new_bounds.x_max) / 2.0;
                let y_center_old = (old_bounds.y_min + old_bounds.y_max) / 2.0;
                let y_center_new = (new_bounds.y_min + new_bounds.y_max) / 2.0;

                let x_center_change = ((x_center_new - x_center_old) / x_range_old.max(1e-6)).abs();
                let y_center_change = ((y_center_new - y_center_old) / y_range_old.max(1e-6)).abs();

                // Trigger if any change exceeds threshold
                x_change > self.lod_threshold || y_change > self.lod_threshold || x_center_change > self.lod_threshold || y_center_change > self.lod_threshold
            }
        }
    }

    /// Indices-based LOD pipeline for improved performance with static captures
    /// Returns indices into the events slice instead of cloning events
    fn apply_lod_indices(&mut self, events: &[MouseMoveEvent], visible_width: f64, plot_bounds: Option<&PlotBounds>) -> Vec<usize> {
        use std::collections::HashSet;

        if events.is_empty() {
            return Vec::new();
        }

        // Check if cache needs invalidation due to events change
        if events.len() != self.last_events_len {
            self.cached_lod_indices.clear();
            self.lod_pyramid.clear();
            self.last_target_points = None;
            self.last_events_len = events.len();
        }

        // Constants for tuning
        const MARGIN_PX: f64 = 8.0;
        const COLINEARITY_TOL: f64 = 0.6;

        // Calculate target points (2 points per pixel)
        let target_points = (visible_width * 2.0) as usize;

        // Binary search to find visible slice
        let (start_idx, end_idx) = if let Some(bounds) = plot_bounds {
            // Add margin in time units
            let time_range = bounds.x_max - bounds.x_min;
            let margin_time = if time_range > 0.0 { (MARGIN_PX / visible_width) * time_range } else { 0.0 };

            let x_min_with_margin = bounds.x_min - margin_time;
            let x_max_with_margin = bounds.x_max + margin_time;

            // Use partition_point for binary search
            let start = events.partition_point(|e| e.time < x_min_with_margin);
            let end = events.partition_point(|e| e.time <= x_max_with_margin);

            (start, end)
        } else {
            (0, events.len())
        };

        let visible_count = end_idx.saturating_sub(start_idx);

        if visible_count == 0 {
            return Vec::new();
        }

        if visible_count <= target_points {
            // No downsampling needed
            return (start_idx..end_idx).collect();
        }

        // Check for coarsen-from-previous fast path (zooming out)
        if let Some(last_target) = self.last_target_points {
            if target_points < last_target && !self.cached_lod_indices.is_empty() {
                // Coarsen from cached indices
                let step = self.cached_lod_indices.len() / target_points.max(1);
                let step = step.max(1);

                let coarsened: Vec<usize> = self.cached_lod_indices.iter().step_by(step).copied().collect();

                self.last_target_points = Some(target_points);
                return coarsened;
            }
        }

        // Full LOD computation with trend preservation
        let mut selected_indices = Vec::with_capacity(target_points);
        let mut dedup_set = HashSet::new();

        // Calculate bucket size
        let bucket_size = visible_count / target_points.max(1);
        let bucket_size = bucket_size.max(1);

        for bucket_start in (start_idx..end_idx).step_by(bucket_size) {
            let bucket_end = (bucket_start + bucket_size).min(end_idx);

            if bucket_start >= bucket_end {
                continue;
            }

            // Collect indices for this bucket with trend preservation
            // Include first and last of bucket
            let first_idx = bucket_start;
            let last_idx = bucket_end - 1;

            // Find min/max dx and dy in bucket
            let mut min_dx_idx = first_idx;
            let mut max_dx_idx = first_idx;
            let mut min_dy_idx = first_idx;
            let mut max_dy_idx = first_idx;

            for idx in bucket_start..bucket_end {
                if events[idx].dx < events[min_dx_idx].dx {
                    min_dx_idx = idx;
                }
                if events[idx].dx > events[max_dx_idx].dx {
                    max_dx_idx = idx;
                }
                if events[idx].dy < events[min_dy_idx].dy {
                    min_dy_idx = idx;
                }
                if events[idx].dy > events[max_dy_idx].dy {
                    max_dy_idx = idx;
                }
            }

            // Add unique indices using time deduplication
            for &idx in &[first_idx, last_idx, min_dx_idx, max_dx_idx, min_dy_idx, max_dy_idx] {
                let time_bits = events[idx].time.to_bits();
                if dedup_set.insert(time_bits) {
                    selected_indices.push(idx);
                }
            }
        }

        // Sort indices to maintain time order
        selected_indices.sort_unstable();

        // Validate all indices are within bounds
        debug_assert!(selected_indices.iter().all(|&idx| idx < events.len()), "All indices should be within events bounds");

        // Update cache
        self.cached_lod_indices = selected_indices.clone();
        self.last_target_points = Some(target_points);

        selected_indices
    }

    fn calculate_stats(&self, events: &[MouseMoveEvent]) -> Stats {
        if events.is_empty() {
            return Stats::default();
        }

        let count = events.len();
        let time_start = events.iter().map(|e| e.time).fold(f64::INFINITY, |a, b| a.min(b));
        let time_end = events.iter().map(|e| e.time).fold(f64::NEG_INFINITY, |a, b| a.max(b));
        let duration = (time_end - time_start).max(0.0);

        let total_dx: i64 = events.iter().map(|e| e.dx as i64).sum();
        let total_dy: i64 = events.iter().map(|e| e.dy as i64).sum();

        let mut total_distance = 0f64;
        let mut magnitudes: Vec<f64> = Vec::with_capacity(count);
        for e in events {
            let mag = ((e.dx as f64).powi(2) + (e.dy as f64).powi(2)).sqrt();
            magnitudes.push(mag);
            total_distance += mag;
        }

        let avg_distance_per_event = total_distance / (count as f64);
        let avg_speed = if duration > 0.0 { total_distance / duration } else { 0.0 };
        let events_per_sec = if duration > 0.0 { count as f64 / duration } else { 0.0 };

        // Calculate histogram
        let max_mag = magnitudes.iter().copied().fold(0.0f64, |a, b| a.max(b));
        let bucket_count = 12usize;
        let mut histogram = vec![0usize; bucket_count];
        let bucket_size = if max_mag <= 0.0 { 1.0 } else { max_mag / (bucket_count as f64) };

        for &m in &magnitudes {
            let idx = if bucket_size == 0.0 {
                0
            } else {
                let v = (m / bucket_size).floor() as isize;
                let v = v.max(0).min((bucket_count - 1) as isize);
                v as usize
            };
            histogram[idx] += 1;
        }

        Stats {
            count,
            duration,
            total_dx,
            total_dy,
            total_distance,
            avg_distance_per_event,
            avg_speed,
            events_per_sec,
            histogram,
            bucket_size,
        }
    }
}

#[derive(Default)]
struct Stats {
    count: usize,
    duration: f64,
    total_dx: i64,
    total_dy: i64,
    total_distance: f64,
    avg_distance_per_event: f64,
    avg_speed: f64,
    events_per_sec: f64,
    histogram: Vec<usize>,
    bucket_size: f64,
}

impl eframe::App for MouseAnalyzerGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check F2 key state for edge detection (transition from not pressed to pressed)
        #[cfg(windows)]
        let f2_pressed_now = key_utils::is_f2_pressed();
        #[cfg(not(windows))]
        let f2_pressed_now = false;

        // Detect F2 key press (rising edge)
        let f2_just_pressed = f2_pressed_now && !self.last_f2_state;
        self.last_f2_state = f2_pressed_now;

        // Handle F2 key press
        if f2_just_pressed {
            if self.is_capturing {
                // Stop current capture and take snapshot
                println!("F2 pressed: stopping capture and drawing plot...");
                self.stop_flag.store(true, Ordering::SeqCst);
                self.captured_events = self.events.lock().unwrap().clone();
                self.is_capturing = false;
                // Clear LOD cache since we have new data
                self.cached_lod_indices.clear();
                self.lod_pyramid.clear();
                self.last_plot_bounds = None;
                self.last_target_points = None;
                self.last_events_len = self.captured_events.len();
            } else {
                // Start a new capture
                println!("F2 pressed: starting new capture...");
                // Clear previous data
                self.events.lock().unwrap().clear();
                self.captured_events.clear();
                self.cached_lod_indices.clear();
                self.lod_pyramid.clear();
                self.last_plot_bounds = None;
                self.last_target_points = None;
                self.last_events_len = 0;
                // Reset stop flag and restart capture
                self.stop_flag.store(false, Ordering::SeqCst);
                self.is_capturing = true;

                // Spawn new capture thread
                let events_capture = Arc::clone(&self.events);
                let stop_capture = Arc::clone(&self.stop_flag);
                let target_device = self.target_device;
                thread::spawn(move || {
                    // Disable F2 watcher in GUI mode since GUI handles F2 itself
                    if let Err(e) = crate::run_capture(events_capture, stop_capture, target_device, true) {
                        eprintln!("Capture error: {}", e);
                    }
                });
            }
        }

        // Only request repaint if we're capturing (to show live event count)
        // When not capturing, we only repaint when needed (user interaction)
        if self.is_capturing {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        // Use appropriate event data source
        let display_events = if self.is_capturing {
            // During capture, don't show plot (deferred plotting)
            Vec::new()
        } else {
            // After capture, show the captured snapshot
            self.captured_events.clone()
        };

        // Calculate stats (use live events for counting during capture)
        let live_events = self.events.lock().unwrap().clone();
        let count_for_display = if self.is_capturing { live_events.len() } else { display_events.len() };
        let stats = self.calculate_stats(&display_events);

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("🖱 Mouse Event Analyzer");
                ui.separator();

                if self.is_capturing {
                    ui.label(format!("Capturing: {} events", count_for_display));
                } else {
                    ui.label(format!("Events: {}", stats.count));
                    if stats.duration > 0.0 {
                        ui.label(format!("Duration: {:.2}s", stats.duration));
                    }
                }
            });
        });

        egui::SidePanel::left("side_panel").min_width(200.0).show(ctx, |ui| {
            ui.heading("Controls");
            ui.separator();

            ui.checkbox(&mut self.show_plot, "Show Plot");
            ui.checkbox(&mut self.show_stats, "Show Statistics");
            ui.checkbox(&mut self.show_histogram, "Show Histogram");
            ui.checkbox(&mut self.show_events_table, "Show Events Table");

            ui.separator();

            if self.is_capturing {
                ui.colored_label(egui::Color32::GREEN, "● Recording");
                ui.label(format!("{} events captured", count_for_display));
                ui.label("Press F2 to stop and plot");
            } else {
                ui.colored_label(egui::Color32::RED, "● Stopped");
                ui.label(format!("{} events total", stats.count));
                ui.label("Press F2 to start new capture");
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                if self.is_capturing {
                    // Show waiting message during capture
                    ui.vertical_centered(|ui| {
                        ui.add_space(50.0);
                        ui.heading("Capturing mouse events...");
                        ui.label(format!("{} events captured", count_for_display));
                        ui.add_space(20.0);
                        ui.label("Move your mouse to record events");
                        ui.label("Press F2 to stop capture and display results");
                    });
                } else if display_events.is_empty() {
                    // Show message when no capture has been done yet
                    ui.vertical_centered(|ui| {
                        ui.add_space(50.0);
                        ui.heading("Ready to capture");
                        ui.label("Press F2 to start capturing (wait for capture thread to initialize)");
                    });
                } else {
                    // Show statistics and plots after capture is stopped
                    if self.show_stats {
                        ui.group(|ui| {
                            ui.heading("Statistics");
                            ui.separator();

                            egui::Grid::new("stats_grid").num_columns(2).spacing([40.0, 4.0]).striped(true).show(ui, |ui| {
                                ui.label("Event Count:");
                                ui.label(format!("{}", stats.count));
                                ui.end_row();

                                ui.label("Duration:");
                                ui.label(format!("{:.6} s", stats.duration));
                                ui.end_row();

                                ui.label("Total dx:");
                                ui.label(format!("{}", stats.total_dx));
                                ui.end_row();

                                ui.label("Total dy:");
                                ui.label(format!("{}", stats.total_dy));
                                ui.end_row();

                                ui.label("Total Distance:");
                                ui.label(format!("{:.3}", stats.total_distance));
                                ui.end_row();

                                ui.label("Avg Distance/Event:");
                                ui.label(format!("{:.3}", stats.avg_distance_per_event));
                                ui.end_row();

                                if stats.duration > 0.0 {
                                    ui.label("Events/sec:");
                                    ui.label(format!("{:.3}", stats.events_per_sec));
                                    ui.end_row();

                                    ui.label("Avg Speed:");
                                    ui.label(format!("{:.3} units/s", stats.avg_speed));
                                    ui.end_row();
                                }
                            });
                        });
                        ui.add_space(10.0);
                    }

                    if self.show_plot {
                        ui.group(|ui| {
                            ui.heading("Movement Plot (dx and -dy vs time)");
                            ui.separator();

                            use egui_plot::{Line, Plot, PlotPoints};

                            // Get screen width for LOD calculation
                            let available_width = ui.available_width();

                            // Show the plot and capture its response to get bounds
                            let plot_response = Plot::new("mouse_plot").view_aspect(2.0).legend(egui_plot::Legend::default()).show(ui, |plot_ui| {
                                // Get current plot bounds
                                let bounds = plot_ui.plot_bounds();
                                let current_bounds = PlotBounds {
                                    x_min: bounds.min()[0],
                                    x_max: bounds.max()[0],
                                    y_min: bounds.min()[1],
                                    y_max: bounds.max()[1],
                                };

                                // Check if we need to recalculate LOD
                                let needs_lod_update = self.bounds_changed_significantly(&current_bounds) || self.cached_lod_indices.is_empty();

                                let lod_indices = if needs_lod_update {
                                    // Recalculate LOD with current bounds
                                    self.apply_lod_indices(&display_events, available_width as f64, Some(&current_bounds))
                                } else {
                                    // Use cached LOD indices
                                    self.cached_lod_indices.clone()
                                };

                                // Helper to safely map indices to plot points
                                let map_to_points = |indices: &[usize], map_fn: fn(&MouseMoveEvent) -> [f64; 2]| indices.iter().filter_map(|&idx| if idx < display_events.len() { Some(map_fn(&display_events[idx])) } else { None }).collect::<PlotPoints>();

                                // Build plot lines by mapping indices to events
                                let dx_points = map_to_points(&lod_indices, |e| [e.time, e.dx as f64]);
                                let dx_line = Line::new(dx_points).color(egui::Color32::from_rgb(255, 0, 0)).name("dx");

                                let ndy_points = map_to_points(&lod_indices, |e| [e.time, -(e.dy as f64)]);
                                let ndy_line = Line::new(ndy_points).color(egui::Color32::from_rgb(0, 0, 255)).name("-dy");

                                plot_ui.line(dx_line);
                                plot_ui.line(ndy_line);

                                (current_bounds, lod_indices, needs_lod_update)
                            });

                            // Update cached values if LOD was recalculated
                            let (current_bounds, lod_indices, needs_lod_update) = plot_response.inner;
                            if needs_lod_update {
                                self.cached_lod_indices = lod_indices.clone();
                                self.last_plot_bounds = Some(current_bounds);
                            }

                            // Show LOD info if downsampling occurred
                            if lod_indices.len() < display_events.len() {
                                ui.label(format!("Showing {} of {} points (LOD applied)", lod_indices.len(), display_events.len()));
                            }
                        });
                        ui.add_space(10.0);
                    }

                    if self.show_histogram {
                        ui.group(|ui| {
                            ui.heading("Movement Magnitude Histogram");
                            ui.separator();

                            use egui_plot::{Bar, BarChart, Plot};

                            let bars: Vec<Bar> = stats
                                .histogram
                                .iter()
                                .enumerate()
                                .map(|(i, &count)| Bar::new(i as f64, count as f64).width(0.8).name(format!("[{:.1}-{:.1})", stats.bucket_size * i as f64, stats.bucket_size * (i + 1) as f64)))
                                .collect();

                            let chart = BarChart::new(bars).color(egui::Color32::from_rgb(100, 200, 100)).name("Count");

                            Plot::new("histogram_plot").view_aspect(2.0).legend(egui_plot::Legend::default()).show(ui, |plot_ui| {
                                plot_ui.bar_chart(chart);
                            });
                        });
                        ui.add_space(10.0);
                    }

                    if self.show_events_table {
                        ui.group(|ui| {
                            ui.heading("Events Table");
                            ui.separator();

                            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                                egui::Grid::new("events_table").num_columns(4).spacing([10.0, 4.0]).striped(true).show(ui, |ui| {
                                    ui.label("Index");
                                    ui.label("dx");
                                    ui.label("dy");
                                    ui.label("Time (s)");
                                    ui.end_row();

                                    // Show last 100 events or all if less
                                    let start_idx = if display_events.len() > 100 { display_events.len() - 100 } else { 0 };

                                    for (idx, event) in display_events.iter().enumerate().skip(start_idx) {
                                        ui.label(format!("{}", idx));
                                        ui.label(format!("{}", event.dx));
                                        ui.label(format!("{}", event.dy));
                                        ui.label(format!("{:.6}", event.time));
                                        ui.end_row();
                                    }
                                });
                            });

                            if display_events.len() > 100 {
                                ui.label(format!("Showing last 100 of {} events", display_events.len()));
                            }
                        });
                    }
                }
            });
        });
    }
}

pub fn run_gui(events: Arc<Mutex<Vec<MouseMoveEvent>>>, stop_flag: Arc<AtomicBool>, target_device: Option<crate::TargetDevice>) -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, 800.0]).with_title("Mouse Event Analyzer"),
        ..Default::default()
    };

    eframe::run_native("Mouse Event Analyzer", options, Box::new(move |_cc| Box::new(MouseAnalyzerGui::new(events, stop_flag, target_device))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_events() -> Vec<MouseMoveEvent> {
        vec![
            MouseMoveEvent { dx: 10, dy: 5, time: 0.0 },
            MouseMoveEvent { dx: -5, dy: 10, time: 0.1 },
            MouseMoveEvent { dx: 3, dy: -3, time: 0.2 },
            MouseMoveEvent { dx: 0, dy: 0, time: 0.3 },
        ]
    }

    #[test]
    fn test_stats_calculation() {
        let gui = MouseAnalyzerGui::new(Arc::new(Mutex::new(vec![])), Arc::new(AtomicBool::new(false)), None);
        let events = create_test_events();
        let stats = gui.calculate_stats(&events);

        assert_eq!(stats.count, 4);
        assert_eq!(stats.duration, 0.3);
        assert_eq!(stats.total_dx, 8);
        assert_eq!(stats.total_dy, 12);

        // Check that distance is calculated
        assert!(stats.total_distance > 0.0);
        assert!(stats.avg_distance_per_event > 0.0);
    }

    #[test]
    fn test_empty_events() {
        let gui = MouseAnalyzerGui::new(Arc::new(Mutex::new(vec![])), Arc::new(AtomicBool::new(false)), None);
        let stats = gui.calculate_stats(&[]);

        assert_eq!(stats.count, 0);
        assert_eq!(stats.duration, 0.0);
        assert_eq!(stats.total_dx, 0);
        assert_eq!(stats.total_dy, 0);
    }

    #[test]
    fn test_histogram_generation() {
        let gui = MouseAnalyzerGui::new(Arc::new(Mutex::new(vec![])), Arc::new(AtomicBool::new(false)), None);
        let events = create_test_events();
        let stats = gui.calculate_stats(&events);

        assert_eq!(stats.histogram.len(), 12);

        // At least one bucket should have events
        let total_in_histogram: usize = stats.histogram.iter().sum();
        assert_eq!(total_in_histogram, events.len());
    }

    #[test]
    fn test_lod_indices_no_downsampling() {
        let mut gui = MouseAnalyzerGui::new(Arc::new(Mutex::new(vec![])), Arc::new(AtomicBool::new(false)), None);
        let events = create_test_events();

        // With large visible width, no downsampling should occur
        let lod_indices = gui.apply_lod_indices(&events, 1000.0, None);
        assert_eq!(lod_indices.len(), events.len());

        // Indices should be in order
        for (i, &idx) in lod_indices.iter().enumerate() {
            assert_eq!(idx, i);
        }
    }

    #[test]
    fn test_lod_indices_with_downsampling() {
        let mut gui = MouseAnalyzerGui::new(Arc::new(Mutex::new(vec![])), Arc::new(AtomicBool::new(false)), None);

        // Create many events
        let mut many_events = Vec::new();
        for i in 0..1000 {
            many_events.push(MouseMoveEvent {
                dx: (i % 10) as i16,
                dy: (i % 5) as i16,
                time: i as f64 * 0.01,
            });
        }

        // With small visible width, downsampling should occur
        let lod_indices = gui.apply_lod_indices(&many_events, 100.0, None);

        // Should be downsampled (target is 2 * visible_width = 200)
        assert!(lod_indices.len() < many_events.len());
        assert!(lod_indices.len() > 0);
        assert!(lod_indices.len() <= 200 * 6); // Max 6 points per bucket (first/last + min/max dx/dy)

        // Indices should be sorted
        for i in 1..lod_indices.len() {
            assert!(lod_indices[i] >= lod_indices[i - 1]);
        }

        // All indices should be valid
        for &idx in &lod_indices {
            assert!(idx < many_events.len());
        }
    }

    #[test]
    fn test_lod_indices_empty_events() {
        let mut gui = MouseAnalyzerGui::new(Arc::new(Mutex::new(vec![])), Arc::new(AtomicBool::new(false)), None);

        let lod_indices = gui.apply_lod_indices(&[], 100.0, None);
        assert_eq!(lod_indices.len(), 0);
    }

    #[test]
    fn test_lod_indices_with_bounds_filtering() {
        let mut gui = MouseAnalyzerGui::new(Arc::new(Mutex::new(vec![])), Arc::new(AtomicBool::new(false)), None);

        // Create events with times from 0.0 to 9.9
        let mut events = Vec::new();
        for i in 0..100 {
            events.push(MouseMoveEvent {
                dx: (i % 10) as i16,
                dy: (i % 5) as i16,
                time: i as f64 * 0.1,
            });
        }

        // Create bounds that only include times 2.0 to 5.0
        let bounds = PlotBounds {
            x_min: 2.0,
            x_max: 5.0,
            y_min: -10.0,
            y_max: 10.0,
        };

        let lod_indices = gui.apply_lod_indices(&events, 1000.0, Some(&bounds));

        // Should only include events within the bounds (with margin)
        assert!(lod_indices.len() < events.len());
        for &idx in &lod_indices {
            let event_time = events[idx].time;
            // Account for margin (8px worth of time)
            assert!(event_time >= 1.9 && event_time <= 5.1, "Event time {} outside expected range", event_time);
        }
    }

    #[test]
    fn test_bounds_changed_significantly_first_time() {
        let gui = MouseAnalyzerGui::new(Arc::new(Mutex::new(vec![])), Arc::new(AtomicBool::new(false)), None);

        let bounds = PlotBounds {
            x_min: 0.0,
            x_max: 10.0,
            y_min: -5.0,
            y_max: 5.0,
        };

        // First time should always return true
        assert!(gui.bounds_changed_significantly(&bounds));
    }

    #[test]
    fn test_bounds_changed_significantly_small_change() {
        let mut gui = MouseAnalyzerGui::new(Arc::new(Mutex::new(vec![])), Arc::new(AtomicBool::new(false)), None);

        let initial_bounds = PlotBounds {
            x_min: 0.0,
            x_max: 10.0,
            y_min: -5.0,
            y_max: 5.0,
        };
        gui.last_plot_bounds = Some(initial_bounds);

        // Very small change (5% zoom)
        let new_bounds = PlotBounds {
            x_min: 0.25,
            x_max: 9.75,
            y_min: -4.75,
            y_max: 4.75,
        };

        // Should not trigger with default 10% threshold
        assert!(!gui.bounds_changed_significantly(&new_bounds));
    }

    #[test]
    fn test_bounds_changed_significantly_large_zoom() {
        let mut gui = MouseAnalyzerGui::new(Arc::new(Mutex::new(vec![])), Arc::new(AtomicBool::new(false)), None);

        let initial_bounds = PlotBounds {
            x_min: 0.0,
            x_max: 10.0,
            y_min: -5.0,
            y_max: 5.0,
        };
        gui.last_plot_bounds = Some(initial_bounds);

        // Large zoom change (50% zoom in)
        let new_bounds = PlotBounds { x_min: 2.5, x_max: 7.5, y_min: -2.5, y_max: 2.5 };

        // Should trigger update
        assert!(gui.bounds_changed_significantly(&new_bounds));
    }

    #[test]
    fn test_bounds_changed_significantly_pan() {
        let mut gui = MouseAnalyzerGui::new(Arc::new(Mutex::new(vec![])), Arc::new(AtomicBool::new(false)), None);

        let initial_bounds = PlotBounds {
            x_min: 0.0,
            x_max: 10.0,
            y_min: -5.0,
            y_max: 5.0,
        };
        gui.last_plot_bounds = Some(initial_bounds);

        // Significant pan (more than 10% of range)
        let new_bounds = PlotBounds {
            x_min: 2.0,
            x_max: 12.0,
            y_min: -5.0,
            y_max: 5.0,
        };

        // Should trigger update
        assert!(gui.bounds_changed_significantly(&new_bounds));
    }

    #[test]
    fn test_lod_performance_64k() {
        let mut gui = MouseAnalyzerGui::new(Arc::new(Mutex::new(vec![])), Arc::new(AtomicBool::new(false)), None);

        // Create 64,000 synthetic events with patterns
        let mut events = Vec::new();
        for i in 0..64_000 {
            events.push(MouseMoveEvent {
                dx: ((i % 100) as i16) - 50, // Pattern from -50 to 49
                dy: ((i % 50) as i16) - 25,  // Pattern from -25 to 24
                time: i as f64 * 0.001,      // Time increasing: 0.0 to 63.999s
            });
        }

        // Simulate a visible window covering a portion of the time range
        let bounds = PlotBounds {
            x_min: 10.0,
            x_max: 20.0,
            y_min: -100.0,
            y_max: 100.0,
        };

        let visible_width = 1000.0;
        let target_points = (visible_width * 2.0) as usize;

        let lod_indices = gui.apply_lod_indices(&events, visible_width, Some(&bounds));

        // Verify returned indices count is reasonable
        assert!(lod_indices.len() > 0, "Should return some indices");
        assert!(lod_indices.len() <= target_points * 6, "Should not exceed max possible indices");

        // Verify all indices are within bounds
        for &idx in &lod_indices {
            assert!(idx < events.len(), "Index {} out of bounds", idx);
        }

        // Verify indices are sorted by time (should be sorted by index)
        for i in 1..lod_indices.len() {
            assert!(lod_indices[i] >= lod_indices[i - 1], "Indices should be sorted");
            assert!(events[lod_indices[i]].time >= events[lod_indices[i - 1]].time, "Events should be sorted by time");
        }

        // Verify events are within visible bounds (with margin)
        for &idx in &lod_indices {
            let event_time = events[idx].time;
            // With margin, should be approximately within bounds
            assert!(event_time >= 9.9 && event_time <= 20.1, "Event time {} should be approximately within bounds", event_time);
        }
    }

    #[test]
    fn test_lod_coarsen_from_previous() {
        let mut gui = MouseAnalyzerGui::new(Arc::new(Mutex::new(vec![])), Arc::new(AtomicBool::new(false)), None);

        // Create test events
        let mut events = Vec::new();
        for i in 0..10_000 {
            events.push(MouseMoveEvent {
                dx: (i % 50) as i16,
                dy: (i % 30) as i16,
                time: i as f64 * 0.001,
            });
        }

        // First call with large visible width (less aggressive LOD)
        let visible_width_1 = 2000.0;
        let indices_1 = gui.apply_lod_indices(&events, visible_width_1, None);
        let count_1 = indices_1.len();

        // Second call with smaller visible width (more aggressive LOD - zooming out)
        let visible_width_2 = 500.0;
        let indices_2 = gui.apply_lod_indices(&events, visible_width_2, None);
        let count_2 = indices_2.len();

        // Should use coarsen-from-previous path and return fewer points
        assert!(count_2 < count_1, "Second call should return fewer indices (coarser): {} vs {}", count_2, count_1);
        assert!(count_2 > 0, "Should still return some indices");

        // Verify indices are still sorted
        for i in 1..indices_2.len() {
            assert!(indices_2[i] >= indices_2[i - 1]);
        }

        // Third call with even smaller width
        let visible_width_3 = 100.0;
        let indices_3 = gui.apply_lod_indices(&events, visible_width_3, None);
        let count_3 = indices_3.len();

        // Should continue to get coarser
        assert!(count_3 <= count_2, "Third call should return same or fewer indices: {} vs {}", count_3, count_2);
    }

    #[test]
    fn test_lod_cache_invalidation() {
        let mut gui = MouseAnalyzerGui::new(Arc::new(Mutex::new(vec![])), Arc::new(AtomicBool::new(false)), None);

        // Create initial events
        let mut events = Vec::new();
        for i in 0..1000 {
            events.push(MouseMoveEvent {
                dx: (i % 10) as i16,
                dy: (i % 5) as i16,
                time: i as f64 * 0.01,
            });
        }

        // First call to populate cache
        let indices_1 = gui.apply_lod_indices(&events, 100.0, None);
        assert!(!indices_1.is_empty());
        assert!(!gui.cached_lod_indices.is_empty());

        // Create new events with different length (simulate capture reset)
        let mut new_events = Vec::new();
        for i in 0..500 {
            new_events.push(MouseMoveEvent {
                dx: (i % 15) as i16,
                dy: (i % 8) as i16,
                time: i as f64 * 0.02,
            });
        }

        // Call with new events - cache should be invalidated
        let indices_2 = gui.apply_lod_indices(&new_events, 100.0, None);

        // Should work correctly with new data
        assert!(!indices_2.is_empty());
        for &idx in &indices_2 {
            assert!(idx < new_events.len());
        }
    }
}
