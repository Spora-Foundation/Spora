extern crate self as spora_core;

pub mod assert;
pub mod console;
pub mod log;
pub mod panic;
pub mod time;
pub mod sporad_env;

cfg_if::cfg_if! {
    if #[cfg(not(target_arch = "wasm32"))] {
        pub mod core;
        pub mod service;
        pub mod signals;
        pub mod task;
    }
}
