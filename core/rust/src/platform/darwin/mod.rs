#![allow(clippy::let_unit_value)]

#[cfg(all(any(test, feature = "mock"), target_os = "macos"))]
mod main_thread_hack;

pub mod run_loop;
pub(super) mod sys;
pub mod value;
