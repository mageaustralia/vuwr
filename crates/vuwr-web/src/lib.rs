//! vuwr in the browser.
//!
//! There is no browser-specific behaviour here: the same `vuwr-core`
//! session and the same `vuwr-gui` frontend that the desktop build uses,
//! rendered to a canvas. This crate is only the entry point.
//!
//! The app starts with nothing loaded and waits for a file to be dropped
//! in. Nothing is uploaded — the bytes never leave the tab.

#![cfg(target_arch = "wasm32")]

use eframe::wasm_bindgen::{self, prelude::*};
use vuwr_gui::VuwrApp;

/// Start vuwr on the canvas with the given element id.
#[wasm_bindgen]
pub async fn start(canvas_id: String) -> Result<(), JsValue> {
    // Without this, a panic in the browser is an unhelpful "unreachable
    // executed" in the console.
    console_error_panic_hook::set_once();

    let document = web_sys::window()
        .ok_or_else(|| JsValue::from_str("no window"))?
        .document()
        .ok_or_else(|| JsValue::from_str("no document"))?;
    let canvas = document
        .get_element_by_id(&canvas_id)
        .ok_or_else(|| JsValue::from_str("canvas not found"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;

    eframe::WebRunner::new()
        .start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(|cc| Ok(Box::new(VuwrApp::with_context(&cc.egui_ctx, None, None)))),
        )
        .await
}
