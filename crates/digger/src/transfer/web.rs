//! The browser's transfer (add-web-build-and-releases, design decision 8):
//! a recording is handed out as a file download, built from a `Blob` and
//! a temporary object URL clicked through a temporary link, and a recording
//! to play is picked through the page's hidden file input (`platform::web`). Nothing is sent
//! anywhere.

use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Blob, BlobPropertyBag, HtmlAnchorElement, Url};

use super::{FileTransfer, TransferError};

/// How long the object URL of a download stays valid, in milliseconds:
/// long enough for every browser to start the download.
const REVOKE_AFTER_MS: i32 = 10_000;

/// The page's downloads. The picker is the page's hidden file input,
/// opened by the play-recording button inside its click (browsers open a
/// picker only from a user gesture); its file arrives as an
/// [`AppEvent::RecordingPicked`](crate::event::AppEvent).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct WebFileTransfer;

/// A browser error as the reason of a refused transfer.
fn refused(error: &JsValue) -> TransferError {
    TransferError::Refused(crate::store::web::describe(error))
}

impl FileTransfer for WebFileTransfer {
    fn export(&mut self, name: &str, bytes: &[u8]) -> Result<(), TransferError> {
        let window = web_sys::window()
            .ok_or_else(|| TransferError::Refused(String::from("there is no browser window")))?;
        let document = window
            .document()
            .ok_or_else(|| TransferError::Refused(String::from("the page has no document")))?;
        let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
        let options = BlobPropertyBag::new();
        options.set_type("application/octet-stream");
        let blob = Blob::new_with_u8_array_sequence_and_options(&parts, &options)
            .map_err(|error| refused(&error))?;
        let url = Url::create_object_url_with_blob(&blob).map_err(|error| refused(&error))?;
        let link = document
            .create_element("a")
            .map_err(|error| refused(&error))?
            .dyn_into::<HtmlAnchorElement>()
            .map_err(|_| TransferError::Refused(String::from("cannot make a link")))?;
        link.set_href(&url);
        link.set_download(name);
        link.set_hidden(true);
        let body = document
            .body()
            .ok_or_else(|| TransferError::Refused(String::from("the page has no body")))?;
        body.append_child(&link).map_err(|error| refused(&error))?;
        link.click();
        let _ = body.remove_child(&link);
        // Revoked later rather than at once: some browsers read the URL
        // after the click has returned.
        let revoke = Closure::once_into_js(move || {
            let _ = Url::revoke_object_url(&url);
        });
        // Without a timer the URL simply lives until the page closes.
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
            revoke.unchecked_ref(),
            REVOKE_AFTER_MS,
        );
        Ok(())
    }
}
