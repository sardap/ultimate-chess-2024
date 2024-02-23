use std::sync::Mutex;

use once_cell::sync::Lazy;
use wasm_mt::{prelude::*, Thread};

pub struct WasmThreadHolder {
    pub thread: Thread,
}

pub static mut WASM_THREAD_HOLDER: Lazy<Mutex<Option<WasmThreadHolder>>> =
    Lazy::new(|| Mutex::new(None));

pub async fn initialize_wasm_thread() {
    if unsafe { WASM_THREAD_HOLDER.lock().unwrap().is_some() } {
        return;
    }

    let pkg_js = "./pkg/uc2024.js";
    let mt: WasmMt = WasmMt::new(pkg_js).and_init().await.unwrap();
    let th: Thread = mt.thread().and_init().await.unwrap();

    unsafe {
        let mut instance = WASM_THREAD_HOLDER.lock().unwrap();
        *instance = Some(WasmThreadHolder { thread: th });
    }
}
