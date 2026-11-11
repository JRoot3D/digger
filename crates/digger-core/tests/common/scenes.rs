//! The golden scenes: fixed pictures drawn through the public rendering and
//! sprite API (design D8).
//!
//! The same file is compiled into the golden-frame test
//! (`crates/digger-core/tests/golden_frames.rs`), which pins the CRC-32 of
//! every scene, and into `cargo xtask render-sheet`, which writes them as
//! PPM files for a person to look at. Keeping one definition guarantees that
//! the pictures reviewed by eye are the pictures whose checksums are pinned.

use digger_core::assets::{ImageId, TitlePicture};
use digger_core::geom::Px;
use digger_core::sprite::{Collisions, SpriteId, SpriteLayer};
use digger_core::video::{Framebuffer, LevelPlan, TextColor};

/// The four text colours, in number order.
const COLORS: [TextColor; 4] = [
    TextColor::Zero,
    TextColor::One,
    TextColor::Two,
    TextColor::Three,
];

/// Level plan 1, the background used by the contact sheet and the scene.
fn plan_one() -> LevelPlan {
    LevelPlan::new(1).unwrap_or_else(|| unreachable!("plan 1 exists"))
}

/// The title picture alone.
pub(crate) fn title(title: &TitlePicture) -> Framebuffer {
    let mut fb = Framebuffer::new();
    fb.draw_title(title);
    fb
}

/// The title picture with "D I G G E R" at logical (100, 0) in colour 3, as
/// the title screen shows it (research 03 §9.1).
pub(crate) fn title_with_text(picture: &TitlePicture) -> Framebuffer {
    let mut fb = title(picture);
    fb.draw_text("D I G G E R", Px::new(100, 0), TextColor::Three);
    fb
}

/// The background of `plan` on a cleared picture.
pub(crate) fn background(plan: LevelPlan) -> Framebuffer {
    let mut fb = Framebuffer::new();
    fb.draw_background(plan);
    fb
}

/// All 66 characters that have a glyph, in each of the four colours: per
/// colour a row of upper-case letters, a row of lower-case letters and a
/// row of digits and punctuation, plus one row with unsupported characters
/// that must draw as spaces.
pub(crate) fn text_sheet() -> Framebuffer {
    const ROWS: [&str; 3] = [
        "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
        "abcdefghijklmnopqrstuvwxyz",
        "0123456789 .:_",
    ];
    let mut fb = Framebuffer::new();
    let mut y = 0;
    for color in COLORS {
        for row in ROWS {
            fb.draw_text(row, Px::new(0, y), color);
            y += 12;
        }
    }
    fb.draw_text("A-B!C/D", Px::new(0, y + 4), TextColor::Three);
    fb
}

/// Numbers in every colour at several widths, including a leading zero
/// (left cell untouched), a zero of width 1 (nothing drawn) and a value
/// wider than its field (high digits dropped).
pub(crate) fn number_sheet() -> Framebuffer {
    const NUMBERS: [(u32, usize); 6] = [
        (0, 6),
        (123_456, 6),
        (7, 1),
        (0, 1),
        (1_234_567, 6),
        (42, 3),
    ];
    let mut fb = Framebuffer::new();
    let mut y = 0;
    for color in COLORS {
        let mut x = 0;
        for (n, width) in NUMBERS {
            fb.draw_number(n, width, Px::new(x, y), color);
            x += 12 * i32::try_from(width).unwrap_or(0) + 4;
        }
        y += 16;
    }
    fb
}

/// Every one of the 120 images, 12 per row in number order, each drawn
/// masked in a 26x20 logical cell over background tile 94 that covers the
/// whole picture.
pub(crate) fn contact_sheet() -> Framebuffer {
    let mut fb = Framebuffer::new();
    let tile = plan_one().tile();
    for y in (0..200).step_by(4) {
        for x in (0..320).step_by(20) {
            fb.blit_masked(tile, Px::new(x, y));
        }
    }
    for image in ImageId::all() {
        let n = i32::from(image.number());
        fb.blit_masked(image, Px::new(n % 12 * 26 + 4, n / 12 * 20 + 2));
    }
    fb
}

fn slot(id: Option<SpriteId>) -> SpriteId {
    id.unwrap_or_else(|| unreachable!("scene slot numbers are in range"))
}

fn image(n: u8) -> ImageId {
    ImageId::new(n).unwrap_or_else(|| unreachable!("scene image {n} is below 120"))
}

/// A scripted sprite scene on the plan 1 background: emeralds, a tunnel
/// carved under a walking digger, two bags, two monsters, a bonus, a
/// fireball that hits a monster, and an erased monster. Returns the picture
/// and the collision lists of the last move.
pub(crate) fn sprite_scene() -> (Framebuffer, Collisions) {
    let mut fb = Framebuffer::new();
    fb.draw_background(plan_one());
    fb.draw_number(0, 5, Px::new(0, 0), TextColor::Zero);
    fb.draw_text("SCENE", Px::new(200, 0), TextColor::Three);

    let mut layer = SpriteLayer::new();
    for (x, y) in [(28, 42), (48, 42), (68, 42), (240, 120), (260, 120)] {
        layer.draw_static_unprotected(&mut fb, ImageId::EMERALD, Px::new(x, y));
    }

    let bag0 = slot(SpriteId::bag(0));
    let bag1 = slot(SpriteId::bag(1));
    let nobbin = slot(SpriteId::monster(0));
    let hobbin = slot(SpriteId::monster(1));
    let fireball = slot(SpriteId::fireball(0));
    let digger = slot(SpriteId::digger(0));
    let bonus = SpriteId::BONUS;

    layer.reset(bag0, ImageId::BAG_STILL);
    layer.place(&mut fb, bag0, Px::new(100, 100));
    layer.reset(bag1, ImageId::GOLD_1);
    layer.place(&mut fb, bag1, Px::new(162, 58));
    layer.reset(nobbin, ImageId::NOBBIN_1);
    layer.place(&mut fb, nobbin, Px::new(40, 140));
    layer.reset(hobbin, ImageId::HOBBIN_LEFT_1);
    layer.place(&mut fb, hobbin, Px::new(200, 140));
    layer.reset(bonus, ImageId::BONUS);
    layer.place(&mut fb, bonus, Px::new(280, 20));

    // The digger walks right along row 180, carving a tunnel under itself.
    layer.reset(digger, image(1));
    layer.place(&mut fb, digger, Px::new(12, 180));
    let mut x = 12;
    for step in 0..20u8 {
        layer.draw_static(&mut fb, ImageId::TUNNEL_RIGHT, Px::new(x + 12, 178));
        x += 4;
        layer.set_pending(digger, image(1 + step % 3));
        layer.move_to(&mut fb, digger, Px::new(x, 180));
    }
    layer.draw_static(&mut fb, ImageId::TUNNEL_SQUARE, Px::new(x, 178));

    // A monster walks onto the bag, then the hobbin is erased.
    for step in 0..6u8 {
        layer.set_pending(nobbin, image(69 + step % 3));
        layer.move_to(
            &mut fb,
            nobbin,
            Px::new(44 + 8 * i32::from(step), 140 - 6 * i32::from(step)),
        );
    }
    layer.erase(&mut fb, hobbin);

    // A fireball flies left into the nobbin.
    layer.reset(fireball, ImageId::FIREBALL_1);
    layer.place(&mut fb, fireball, Px::new(120, 112));
    let mut hits = Collisions::EMPTY;
    for step in 0..4u8 {
        layer.set_pending(fireball, image(82 + step % 3));
        hits = layer.move_to(&mut fb, fireball, Px::new(112 - 8 * i32::from(step), 112));
    }
    (fb, hits)
}
