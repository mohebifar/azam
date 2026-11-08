//! azam: a garment design app — 2D patterns, sewing, a fitted avatar and cloth simulation — with
//! Blender's interface.
//!
//! The binary opens the window (or renders a screenshot headless); the library holds the views so
//! tests can drive them headless.

pub mod app;
pub mod areas;
pub mod autosave;
mod avatar_motion;
mod context_menu;
pub mod decals;
pub mod editors;
pub mod export;
pub mod fabric_library;
pub mod garment3d;
mod icons;
pub mod import;
pub mod modal;
pub mod operators;
mod outliner;
pub mod pattern;
mod pattern_input;
mod pattern_tools;
mod place3d;
pub mod prefs;
pub mod properties;
pub mod redo_panel;
pub mod render_view;
pub mod scene;
mod search;
pub mod selection;
pub mod session;
mod share_view;
mod sidebar;
mod techpack;
mod theme;
mod tooltip;
pub mod units;
pub mod viewport;

pub use app::AzamApp;
