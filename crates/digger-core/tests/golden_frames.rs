//! Golden frame checksums (design D8).
//!
//! Each value is the CRC-32 of the 256000 palette indices of a scene from
//! `common/scenes.rs`. They were recorded from the port after the scenario
//! tests passed and the pictures written by `cargo xtask render-sheet` had
//! been looked at. The title picture's value is independent of the renderer:
//! `cargo xtask import-assets` prints it from the inflated upstream bytes.
//! A failing assertion prints the actual value.

#[path = "common/scenes.rs"]
mod scenes;

use digger_core::assets::TitlePicture;
use digger_core::sprite::{SpriteClass, SpriteId};
use digger_core::video::{Framebuffer, LevelPlan};

/// CRC-32 of 256000 zero bytes, the cleared picture (computed independently
/// with zlib).
const CLEARED: u32 = 0x69FF_78A6;
/// The title picture; equals the value printed by `xtask import-assets`.
const TITLE: u32 = 0x7FA6_3EA5;
/// The title picture with "D I G G E R" at (100, 0) in colour 3.
const TITLE_WITH_TEXT: u32 = 0x297C_CC2F;
/// The backgrounds of level plans 1 to 8 on a cleared picture.
const BACKGROUNDS: [u32; 8] = [
    0x664A_9DF3,
    0x9E48_2D1C,
    0x2F3F_D992,
    0x0195_E702,
    0x2780_CE08,
    0x5DDD_43C8,
    0x2275_AF10,
    0x503D_EE83,
];
/// All 66 characters in four colours.
const TEXT_SHEET: u32 = 0x91C3_C9DE;
/// Numbers in four colours at several widths.
const NUMBER_SHEET: u32 = 0xAD59_37ED;
/// All 120 images over background tile 94.
const CONTACT_SHEET: u32 = 0x4C8F_B6A8;
/// The scripted sprite scene.
const SPRITE_SCENE: u32 = 0x8468_F724;

fn title_picture() -> TitlePicture {
    TitlePicture::decode().unwrap_or_else(|error| unreachable!("title decodes: {error}"))
}

fn assert_crc(name: &str, fb: &Framebuffer, expected: u32) {
    let actual = fb.crc32();
    assert_eq!(
        actual, expected,
        "golden frame {name}: actual CRC-32 0x{actual:08X}, expected 0x{expected:08X}"
    );
}

#[test]
fn golden_cleared_picture() {
    assert_crc("cleared", &Framebuffer::new(), CLEARED);
}

#[test]
fn golden_title_picture() {
    assert_crc("title", &scenes::title(&title_picture()), TITLE);
}

#[test]
fn golden_title_with_text() {
    assert_crc(
        "title with text",
        &scenes::title_with_text(&title_picture()),
        TITLE_WITH_TEXT,
    );
}

#[test]
fn golden_backgrounds() {
    for (plan, expected) in LevelPlan::all().zip(BACKGROUNDS) {
        assert_crc(
            &format!("background {}", plan.number()),
            &scenes::background(plan),
            expected,
        );
    }
}

#[test]
fn golden_text_sheet() {
    assert_crc("text sheet", &scenes::text_sheet(), TEXT_SHEET);
}

#[test]
fn golden_number_sheet() {
    assert_crc("number sheet", &scenes::number_sheet(), NUMBER_SHEET);
}

#[test]
fn golden_contact_sheet() {
    assert_crc("contact sheet", &scenes::contact_sheet(), CONTACT_SHEET);
}

#[test]
fn golden_sprite_scene() {
    let (fb, hits) = scenes::sprite_scene();
    assert_crc("sprite scene", &fb, SPRITE_SCENE);
    // The fireball's last move ends on the nobbin in monster slot 8.
    let monsters: Vec<u8> = hits
        .of(SpriteClass::Monster)
        .map(SpriteId::number)
        .collect();
    assert_eq!(monsters, [8]);
}
