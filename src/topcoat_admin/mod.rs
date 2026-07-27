//! Topcoat-based admin UI for RCPA.
//!
//! This module provides a server-rendered admin interface using the Topcoat
//! framework. It bridges to the existing axum-based API routes via TowerRoute.

pub mod api;
pub mod app;
pub mod pages;

pub use app::build_topcoat_app;
