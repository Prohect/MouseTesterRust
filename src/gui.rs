use eframe::egui;
use std::sync::{Arc, Mutex};
use crate::MouseMoveEvent;

pub struct MouseAnalyzerGui {
    events: Arc<Mutex<Vec<MouseMoveEvent>>>,
    show_plot: bool,
    show_stats: bool,
    show_histogram: bool,
    show_events_table: bool,
}

impl MouseAnalyzerGui {
    pub fn new(events: Arc<Mutex<Vec<MouseMoveEvent>>>) -> Self {
        Self {
            events,
            show_plot: true,
            show_stats: true,
            show_histogram: true,
            show_events_table: false,
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
        // Request repaint to keep updating
        ctx.request_repaint();

        let events = self.events.lock().unwrap().clone();
        let stats = self.calculate_stats(&events);

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("🖱 Mouse Event Analyzer");
                ui.separator();
                ui.label(format!("Events: {}", stats.count));
                if stats.duration > 0.0 {
                    ui.label(format!("Duration: {:.2}s", stats.duration));
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
            ui.label(format!("Recording: {} events", stats.count));
            ui.label("Press F2 to stop recording");
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                if self.show_stats && !events.is_empty() {
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

                if self.show_plot && !events.is_empty() {
                    ui.group(|ui| {
                        ui.heading("Movement Plot (dx and -dy vs time)");
                        ui.separator();
                        
                        use egui_plot::{Line, Plot, PlotPoints};
                        
                        let dx_points: PlotPoints = events
                            .iter()
                            .map(|e| [e.time, e.dx as f64])
                            .collect();
                        let dx_line = Line::new(dx_points)
                            .color(egui::Color32::from_rgb(255, 0, 0))
                            .name("dx");

                        let ndy_points: PlotPoints = events
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
                    });
                    ui.add_space(10.0);
                }

                if self.show_histogram && !events.is_empty() {
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

                if self.show_events_table && !events.is_empty() {
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
                                        let start_idx = if events.len() > 100 {
                                            events.len() - 100
                                        } else {
                                            0
                                        };

                                        for (idx, event) in events.iter().enumerate().skip(start_idx) {
                                            ui.label(format!("{}", idx));
                                            ui.label(format!("{}", event.dx));
                                            ui.label(format!("{}", event.dy));
                                            ui.label(format!("{:.6}", event.time));
                                            ui.end_row();
                                        }
                                    });
                            });
                        
                        if events.len() > 100 {
                            ui.label(format!("Showing last 100 of {} events", events.len()));
                        }
                    });
                }

                if events.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(50.0);
                        ui.heading("No events recorded yet");
                        ui.label("Waiting for mouse events...");
                        ui.label("Press F2 to stop recording and analyze");
                    });
                }
            });
        });
    }
}

pub fn run_gui(events: Arc<Mutex<Vec<MouseMoveEvent>>>) -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("Mouse Event Analyzer"),
        ..Default::default()
    };
    
    eframe::run_native(
        "Mouse Event Analyzer",
        options,
        Box::new(|_cc| Box::new(MouseAnalyzerGui::new(events))),
    )
}
