//! vuwr in the browser.
//!
//! There is no browser-specific behaviour here: the same `vuwr-core`
//! session and the same `vuwr-gui` frontend that the desktop build uses,
//! rendered to a canvas. This crate is only the entry point.
//!
//! The app starts with nothing loaded and waits for a file to be dropped
//! in. Nothing is uploaded — the bytes never leave the tab. A `?sample`
//! in the address opens a small bundled document instead, so the hosted
//! demo has something in it for somebody arriving with no file to hand.

#![cfg(target_arch = "wasm32")]

use eframe::wasm_bindgen::{self, prelude::*};
use vuwr_core::{Document, FormatHint};
use vuwr_gui::VuwrApp;

/// A document to open when the page is asked for one. Compiled in rather
/// than fetched: the point of this build is that it needs no server.
const SAMPLES: [(&str, &str, FormatHint); 3] = [
    (
        "xml",
        include_str!("../../../examples/products.xml"),
        FormatHint::Xml,
    ),
    (
        "csv",
        include_str!("../../../examples/stock.csv"),
        FormatHint::Csv,
    ),
    (
        "json",
        include_str!("../../../examples/settings.json"),
        FormatHint::Json,
    ),
];

/// The sample named in the query string, if there is one: `?sample` for
/// the first, `?sample=csv` for one by name.
fn requested_sample() -> Option<(String, Document)> {
    let search = web_sys::window()?.location().search().ok()?;
    let query = search.trim_start_matches('?');
    let asked = query.split('&').find(|p| p.starts_with("sample"))?;
    let name = asked.split_once('=').map(|(_, v)| v).unwrap_or("xml");
    let (name, text, hint) = SAMPLES
        .iter()
        .find(|(n, ..)| *n == name)
        .copied()
        .unwrap_or(SAMPLES[0]);
    let doc = Document::parse(text.as_bytes(), hint).ok()?;
    Some((format!("sample.{name}"), doc))
}

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
            Box::new(|cc| {
                let (path, doc) = match requested_sample() {
                    Some((name, doc)) => (Some(name.into()), Some(doc)),
                    None => (None, None),
                };
                Ok(Box::new(VuwrApp::with_context(&cc.egui_ctx, path, doc)))
            }),
        )
        .await
}
