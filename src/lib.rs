//! MouseTesterRust library
//!
//! This library provides modules for processing and analyzing USB mouse movement data
//! captured via pcap. It includes:
//!
//! - `mouse_event`: Data structures and parsers for mouse movement events with pcap timestamps
//! - `lod`: Level-of-detail hierarchical segmentation for efficient offline data visualization
//! - `lod_advanced`: Advanced LOD with time consistency analysis and adaptive segmentation
//!
//! # Example
//!
//! ```rust,ignore
//! use MouseTesterRust::mouse_event::MouseMoveEvent;
//! use MouseTesterRust::lod::{build_segment_tree, collect_for_view};
//!
//! // Create or capture events
//! let events: Vec<MouseMoveEvent> = vec![/* ... */];
//!
//! // Build LOD tree for efficient visualization
//! let tree = build_segment_tree(&events, 0, events.len(), 5, 1000, 1.0, 1.0);
//!
//! // Collect points for a specific view
//! let mut view_points = Vec::new();
//! collect_for_view(&tree, &events, 1.0, 0.5, &mut view_points);
//! ```

pub mod lod;
pub mod lod_advanced;
pub mod mouse_event;
