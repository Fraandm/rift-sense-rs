//! RiftSense reescrito en Rust.
//!
//! Fase 1: runtime (datos, motor ONNX, API). El pipeline de datos,
//! entrenamiento, escritorio y actualizador se portan en fases posteriores.

pub mod api;
pub mod config;
pub mod data;
pub mod engine;
pub mod lcu;
pub mod model;
