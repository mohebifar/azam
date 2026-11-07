//! azam: a garment design app — 2D patterns, sewing, a fitted avatar and cloth simulation — with
//! Blender's interface.
//!
//! The binary opens the window (or renders a screenshot headless); the library holds the views so
//! tests can drive them headless.

pub mod app;
mod avatar_motion;
mod export;
pub mod garment3d;
mod icons;
pub mod import;
pub mod operators;
mod outliner;
pub mod pattern;
mod pattern_input;
mod pattern_tools;
pub mod properties;
pub mod render_view;
pub mod scene;
pub mod selection;
pub mod session;
mod share_view;
mod techpack;
mod theme;
mod tooltip;
pub mod viewport;

pub use app::AzamApp;
