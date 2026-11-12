//! Pure presentation maths: the scaled viewport and the palette blit
//! (design D5).

pub(crate) mod blit;
#[cfg_attr(
    not(any(test, target_arch = "wasm32")),
    expect(dead_code, reason = "only the browser sizes a canvas")
)]
pub(crate) mod canvas;
pub(crate) mod scale;
