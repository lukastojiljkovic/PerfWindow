//! Library entry point so integration tests under `tests/` can call
//! into the internal modules. The binary entry stays in `main.rs`.

#![allow(dead_code)]

pub mod app;
pub mod config;
pub mod format;
pub mod history;
pub mod ipc;
pub mod panels;
pub mod theme;
pub mod ui;
pub mod update;
pub mod widgets;
