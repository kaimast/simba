#![allow(clippy::bool_to_int_with_if)]
#![allow(clippy::arc_with_non_send_sync)]

pub mod graphics;
pub mod scene;
pub mod ui;
pub mod window_loop;

use std::future::Future;
use tokio::task::JoinHandle;

cfg_if::cfg_if! {
    if #[cfg(target_arch="wasm32")] {
        pub fn spawn_task<F>(future: F) -> JoinHandle<F::Output> where
            F: Future + 'static,
            F::Output: 'static {
            tokio::task::spawn_local(future)
        }
    } else {
        pub fn spawn_task<F>(future: F) -> JoinHandle<F::Output> where
            F: Future + Send + 'static,
            F::Output: Send+ 'static {
            tokio::spawn(future)
        }
    }
}
