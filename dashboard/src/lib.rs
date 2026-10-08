//! Library entry point so integration tests under `tests/` can call
//! into the internal modules. The binary entry stays in `main.rs`.

#![allow(dead_code)]

pub mod app;
pub mod config;
pub mod displays;
pub mod format;
pub mod history;
pub mod ipc;
pub mod panels;
pub mod strip_window;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
pub mod theme;
pub mod ui;
pub mod update;
pub mod widgets;
