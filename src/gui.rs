use eframe::egui;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use crate::MouseMoveEvent;

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
    captured_events: Vec<MouseMoveEvent>, // Events snapshot when capture stopped
    last_f2_state: bool, // For edge detection
}

impl MouseAnalyzerGui {
    pub fn new(events: Arc<Mutex<Vec<MouseMoveEvent>>>, stop_flag: Arc<AtomicBool>) -> Self {
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
        }
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

    /// Downsample events for LOD based on zoom level
    /// Returns a subset of events when there are many points to improve rendering performance
    fn apply_lod(&self, events: &[MouseMoveEvent], visible_width: f64) -> Vec<MouseMoveEvent> {
        if events.is_empty() {
            return Vec::new();
        }

        // Calculate points per pixel as a heuristic for LOD
        // If we have more points than pixels, we should downsample
        let target_points = (visible_width * 2.0) as usize; // 2 points per pixel is plenty
        
        if events.len() <= target_points {
            // No need to downsample
            return events.to_vec();
        }

        // Downsample by taking every nth point
        let step = events.len() / target_points;
        let step = step.max(1);
        
        events.iter()
            .step_by(step)
            .copied()
            .collect()
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

        // Handle F2 key press - stop capture and take snapshot
        if f2_just_pressed && self.is_capturing {
            println!("F2 pressed: stopping capture and drawing plot...");
            self.stop_flag.store(true, Ordering::SeqCst);
            self.captured_events = self.events.lock().unwrap().clone();
            self.is_capturing = false;
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
                ui.label("Analysis complete");
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
                            
                            egui::Grid::new("stats_grid")
                                .num_columns(2)
                                .spacing([40.0, 4.0])
                                .striped(true)
                                .show(ui, |ui| {
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
                            
                            // Apply LOD to reduce point count for better performance
                            let lod_events = self.apply_lod(&display_events, available_width as f64);
                            
                            let dx_points: PlotPoints = lod_events
                                .iter()
                                .map(|e| [e.time, e.dx as f64])
                                .collect();
                            let dx_line = Line::new(dx_points)
                                .color(egui::Color32::from_rgb(255, 0, 0))
                                .name("dx");

                            let ndy_points: PlotPoints = lod_events
                                .iter()
                                .map(|e| [e.time, -(e.dy as f64)])
                                .collect();
                            let ndy_line = Line::new(ndy_points)
                                .color(egui::Color32::from_rgb(0, 0, 255))
                                .name("-dy");

                            Plot::new("mouse_plot")
                                .view_aspect(2.0)
                                .legend(egui_plot::Legend::default())
                                .show(ui, |plot_ui| {
                                    plot_ui.line(dx_line);
                                    plot_ui.line(ndy_line);
                                });
                            
                            // Show LOD info if downsampling occurred
                            if lod_events.len() < display_events.len() {
                                ui.label(format!("Showing {} of {} points (LOD applied)", 
                                    lod_events.len(), display_events.len()));
                            }
                        });
                        ui.add_space(10.0);
                    }

                    if self.show_histogram {
                        ui.group(|ui| {
                            ui.heading("Movement Magnitude Histogram");
                            ui.separator();
                            
                            use egui_plot::{Bar, BarChart, Plot};
                            
                            let bars: Vec<Bar> = stats.histogram
                                .iter()
                                .enumerate()
                                .map(|(i, &count)| {
                                    Bar::new(i as f64, count as f64)
                                        .width(0.8)
                                        .name(format!("[{:.1}-{:.1})", 
                                            stats.bucket_size * i as f64,
                                            stats.bucket_size * (i + 1) as f64))
                                })
                                .collect();

                            let chart = BarChart::new(bars)
                                .color(egui::Color32::from_rgb(100, 200, 100))
                                .name("Count");

                            Plot::new("histogram_plot")
                                .view_aspect(2.0)
                                .legend(egui_plot::Legend::default())
                                .show(ui, |plot_ui| {
                                    plot_ui.bar_chart(chart);
                                });
                        });
                        ui.add_space(10.0);
                    }

                    if self.show_events_table {
                        ui.group(|ui| {
                            ui.heading("Events Table");
                            ui.separator();
                            
                            egui::ScrollArea::vertical()
                                .max_height(300.0)
                                .show(ui, |ui| {
                                    egui::Grid::new("events_table")
                                        .num_columns(4)
                                        .spacing([10.0, 4.0])
                                        .striped(true)
                                        .show(ui, |ui| {
                                            ui.label("Index");
                                            ui.label("dx");
                                            ui.label("dy");
                                            ui.label("Time (s)");
                                            ui.end_row();

                                            // Show last 100 events or all if less
                                            let start_idx = if display_events.len() > 100 {
                                                display_events.len() - 100
                                            } else {
                                                0
                                            };

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

pub fn run_gui(events: Arc<Mutex<Vec<MouseMoveEvent>>>, stop_flag: Arc<AtomicBool>) -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("Mouse Event Analyzer"),
        ..Default::default()
    };
    
    eframe::run_native(
        "Mouse Event Analyzer",
        options,
        Box::new(move |_cc| Box::new(MouseAnalyzerGui::new(events, stop_flag))),
    )
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
        let gui = MouseAnalyzerGui::new(
            Arc::new(Mutex::new(vec![])), 
            Arc::new(AtomicBool::new(false))
        );
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
        let gui = MouseAnalyzerGui::new(
            Arc::new(Mutex::new(vec![])), 
            Arc::new(AtomicBool::new(false))
        );
        let stats = gui.calculate_stats(&[]);
        
        assert_eq!(stats.count, 0);
        assert_eq!(stats.duration, 0.0);
        assert_eq!(stats.total_dx, 0);
        assert_eq!(stats.total_dy, 0);
    }

    #[test]
    fn test_histogram_generation() {
        let gui = MouseAnalyzerGui::new(
            Arc::new(Mutex::new(vec![])), 
            Arc::new(AtomicBool::new(false))
        );
        let events = create_test_events();
        let stats = gui.calculate_stats(&events);

        assert_eq!(stats.histogram.len(), 12);
        
        // At least one bucket should have events
        let total_in_histogram: usize = stats.histogram.iter().sum();
        assert_eq!(total_in_histogram, events.len());
    }

    #[test]
    fn test_lod_no_downsampling() {
        let gui = MouseAnalyzerGui::new(
            Arc::new(Mutex::new(vec![])), 
            Arc::new(AtomicBool::new(false))
        );
        let events = create_test_events();
        
        // With large visible width, no downsampling should occur
        let lod_events = gui.apply_lod(&events, 1000.0);
        assert_eq!(lod_events.len(), events.len());
    }

    #[test]
    fn test_lod_with_downsampling() {
        let gui = MouseAnalyzerGui::new(
            Arc::new(Mutex::new(vec![])), 
            Arc::new(AtomicBool::new(false))
        );
        
        // Create many events
        let mut many_events = Vec::new();
        for i in 0..1000 {
            many_events.push(MouseMoveEvent { 
                dx: (i % 10) as i16, 
                dy: (i % 5) as i16, 
                time: i as f64 * 0.01 
            });
        }
        
        // With small visible width, downsampling should occur
        let lod_events = gui.apply_lod(&many_events, 100.0);
        
        // Should be downsampled (target is 2 * visible_width = 200)
        assert!(lod_events.len() < many_events.len());
        assert!(lod_events.len() > 0);
    }

    #[test]
    fn test_lod_empty_events() {
        let gui = MouseAnalyzerGui::new(
            Arc::new(Mutex::new(vec![])), 
            Arc::new(AtomicBool::new(false))
        );
        
        let lod_events = gui.apply_lod(&[], 100.0);
        assert_eq!(lod_events.len(), 0);
    }
}
