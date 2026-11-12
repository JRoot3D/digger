//! Pure presentation maths: the scaled viewport and the palette blit
//! (design D5).

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the viewport and blit are used by the platform window of a later task group"
    )
)]

pub(crate) mod blit;
pub(crate) mod scale;
