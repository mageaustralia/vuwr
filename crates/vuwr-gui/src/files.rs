//! Opening and saving through the platform's file dialog.
//!
//! `rfd` drives a native dialog on the desktop and a file input in the
//! browser, so "Open" means the same thing in both builds — which is the
//! point: the web version is not a lesser copy.
//!
//! Dialogs are asynchronous everywhere (the browser has no other option),
//! so the result lands in a shared slot the app drains each frame rather
//! than blocking the UI thread.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// A file the user chose. `path` is `None` in the browser, which hands
/// over bytes without a location to write back to.
#[derive(Debug, Clone)]
pub struct Picked {
    pub path: Option<PathBuf>,
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Where a pending dialog leaves its answer.
pub type Pending = Arc<Mutex<Option<Picked>>>;

pub fn pending() -> Pending {
    Arc::new(Mutex::new(None))
}

const EXTENSIONS: &[&str] = &["csv", "tsv", "json", "xml"];

/// Ask for a file to open. Returns immediately; the answer appears in
/// `slot` when the user has chosen.
pub fn open(slot: Pending) {
    let dialog = rfd::AsyncFileDialog::new()
        .add_filter("Data files", EXTENSIONS)
        .add_filter("All files", &["*"]);

    spawn(async move {
        let Some(handle) = dialog.pick_file().await else {
            return;
        };
        let name = handle.file_name();
        let bytes = handle.read().await;
        #[cfg(not(target_arch = "wasm32"))]
        let path = Some(handle.path().to_path_buf());
        #[cfg(target_arch = "wasm32")]
        let path = None;

        if let Ok(mut slot) = slot.lock() {
            *slot = Some(Picked { path, name, bytes });
        }
    });
}

/// Ask where to write. The bytes are captured now, so a slow decision
/// cannot save a later version of the document by surprise.
pub fn save_as(suggested: &str, bytes: Vec<u8>, slot: Arc<Mutex<Option<SaveResult>>>) {
    let dialog = rfd::AsyncFileDialog::new().set_file_name(suggested);

    spawn(async move {
        let Some(handle) = dialog.save_file().await else {
            return;
        };
        let name = handle.file_name();
        let result = match handle.write(&bytes).await {
            Ok(()) => SaveResult::Written {
                #[cfg(not(target_arch = "wasm32"))]
                path: Some(handle.path().to_path_buf()),
                #[cfg(target_arch = "wasm32")]
                path: None,
                name,
            },
            Err(e) => SaveResult::Failed(e.to_string()),
        };
        if let Ok(mut slot) = slot.lock() {
            *slot = Some(result);
        }
    });
}

#[derive(Debug, Clone)]
pub enum SaveResult {
    Written { path: Option<PathBuf>, name: String },
    Failed(String),
}

#[cfg(target_arch = "wasm32")]
fn spawn<F: std::future::Future<Output = ()> + 'static>(f: F) {
    wasm_bindgen_futures::spawn_local(f);
}

/// On the desktop the dialog blocks its own thread, which keeps the UI
/// responsive without pulling in an async runtime.
#[cfg(not(target_arch = "wasm32"))]
fn spawn<F: std::future::Future<Output = ()> + Send + 'static>(f: F) {
    std::thread::spawn(move || pollster::block_on(f));
}
