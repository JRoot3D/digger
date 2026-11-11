//! Unit tests of the sprite layer: one per scenario of the sprite-layer spec,
//! built on a tiled background and checked against pictures composed
//! independently with direct masked draws.

use super::*;
use crate::video::LevelPlan;

const W: usize = Framebuffer::WIDTH;

fn img(n: u8) -> ImageId {
    ImageId::new(n).unwrap_or_else(|| unreachable!("test image number {n} is valid"))
}

fn slot(n: u8) -> SpriteId {
    SpriteId::new(n).unwrap_or_else(|| unreachable!("test slot {n} is valid"))
}

const BAG: u8 = 1;
const MONSTER: u8 = 8;
const DIGGER: u8 = 16;
/// A first-digger walking frame, a masked 4,15 image.
const DIGGER_IMAGE: u8 = 1;

/// The background of level plan 1: tiles everywhere below the status line.
fn background() -> Framebuffer {
    let mut fb = Framebuffer::new();
    fb.draw_background(LevelPlan::new(1).unwrap_or_else(|| unreachable!("plan 1 exists")));
    fb
}

/// Resets `n` to `image` and places it at `at`.
fn show(layer: &mut SpriteLayer, fb: &mut Framebuffer, n: u8, image: u8, at: (i32, i32)) {
    layer.reset(slot(n), img(image));
    layer.place(fb, slot(n), Px::new(at.0, at.1));
}

/// `bg` with every enabled sprite's current image drawn over it in the
/// given slot order.
fn composite_in(bg: &Framebuffer, layer: &SpriteLayer, order: &[SpriteId]) -> Framebuffer {
    let mut fb = bg.clone();
    for &id in order {
        if layer.is_enabled(id) {
            fb.blit_masked(layer.image(id), layer.position(id));
        }
    }
    fb
}

/// `bg` with the enabled sprites drawn over it in ascending slot order.
fn composite(bg: &Framebuffer, layer: &SpriteLayer) -> Framebuffer {
    let order: Vec<SpriteId> = SpriteId::all().collect();
    composite_in(bg, layer, &order)
}

/// The real pixels of a logical rectangle, row by row, clipped.
fn region(fb: &Framebuffer, rect: Rect) -> Vec<u8> {
    let mut out = Vec::new();
    for y in 2 * rect.top()..2 * (rect.bottom() + 1) {
        for x in 2 * rect.left()..2 * (rect.right() + 1) {
            let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
                continue;
            };
            if let Some(&pixel) = fb.pixels().get(y * W + x).filter(|_| x < W) {
                out.push(pixel);
            }
        }
    }
    out
}

fn numbers(ids: impl Iterator<Item = SpriteId>) -> Vec<u8> {
    ids.map(SpriteId::number).collect()
}

fn list(collisions: Collisions, class: SpriteClass) -> Vec<u8> {
    numbers(collisions.of(class))
}

// 7.1 Slots, classes and collision lists as sets.

#[test]
fn sprite_class_ranges() {
    let classes: Vec<SpriteClass> = [0, 7, 8, 13, 14, 17]
        .into_iter()
        .map(|n| slot(n).class())
        .collect();
    assert_eq!(
        classes,
        [
            SpriteClass::Bonus,
            SpriteClass::Bag,
            SpriteClass::Monster,
            SpriteClass::Monster,
            SpriteClass::Fireball,
            SpriteClass::Digger
        ]
    );
    let counts: Vec<usize> = SpriteClass::ALL
        .iter()
        .map(|&class| SpriteId::all().filter(|id| id.class() == class).count())
        .collect();
    assert_eq!(counts, [1, 7, 6, 2, 2]);
    let ranges: Vec<(u8, u8)> = SpriteClass::ALL
        .iter()
        .map(|class| (*class.slot_numbers().start(), *class.slot_numbers().end()))
        .collect();
    assert_eq!(ranges, [(0, 0), (1, 7), (8, 13), (14, 15), (16, 17)]);
    for class in SpriteClass::ALL {
        assert_eq!(class.slots().count(), class.slot_count());
        assert!(class.slots().all(|id| id.class() == class));
    }
    assert_eq!(SpriteId::all().count(), SpriteId::COUNT);
    assert_eq!(numbers(SpriteId::all()), (0..18).collect::<Vec<u8>>());
}

#[test]
fn sprite_id_constructors_reject_out_of_range() {
    type Constructor = fn(u8) -> Option<SpriteId>;

    assert_eq!(SpriteId::new(17).map(SpriteId::number), Some(17));
    assert_eq!(SpriteId::new(18), None);
    assert_eq!(SpriteId::new(255), None);
    assert_eq!(SpriteId::BONUS.number(), 0);
    let cases: [(Constructor, u8, u8); 4] = [
        (SpriteId::bag, 1, 7),
        (SpriteId::monster, 8, 6),
        (SpriteId::fireball, 14, 2),
        (SpriteId::digger, 16, 2),
    ];
    for (constructor, first, count) in cases {
        for i in 0..count {
            assert_eq!(constructor(i).map(SpriteId::number), Some(first + i));
        }
        assert_eq!(constructor(count), None);
        assert_eq!(constructor(255), None);
    }
    assert_eq!(SpriteClass::Bonus.slot(0), Some(SpriteId::BONUS));
    assert_eq!(SpriteClass::Bonus.slot(1), None);
}

#[test]
fn collisions_iterate_ascending_per_class() {
    let mut set = SlotSet::EMPTY;
    for n in [12, 17, 3, 8, 0, 10, 15, 1] {
        set.insert(slot(n));
    }
    let collisions = Collisions::from_set(set);
    assert_eq!(list(collisions, SpriteClass::Bonus), [0]);
    assert_eq!(list(collisions, SpriteClass::Bag), [1, 3]);
    assert_eq!(list(collisions, SpriteClass::Monster), [8, 10, 12]);
    assert_eq!(list(collisions, SpriteClass::Fireball), [15]);
    assert_eq!(list(collisions, SpriteClass::Digger), [17]);
    assert_eq!(
        collisions.first(SpriteClass::Monster).map(SpriteId::number),
        Some(8)
    );
    assert_eq!(numbers(collisions.iter()), [0, 1, 3, 8, 10, 12, 15, 17]);
    assert!(collisions.contains(slot(10)) && !collisions.contains(slot(9)));
    assert!(!collisions.is_empty());
    assert!(Collisions::EMPTY.is_empty());
    assert_eq!(Collisions::default(), Collisions::EMPTY);
    for class in SpriteClass::ALL {
        assert_eq!(Collisions::EMPTY.first(class), None);
    }
}

// 7.2 State, pending images, reset and the overlap test.

#[test]
fn initial_state_all_disabled() {
    let layer = SpriteLayer::new();
    for id in SpriteId::all() {
        assert!(!layer.is_enabled(id));
        assert_eq!(layer.position(id), Px::new(0, 0));
        assert!(layer.saved_background(id).is_none());
    }
    assert!(layer.collisions().is_empty());
    assert_eq!(SpriteLayer::default(), layer);
}

#[test]
fn rect_from_image_size() {
    let mut fb = background();
    let mut layer = SpriteLayer::new();
    show(&mut layer, &mut fb, MONSTER, 69, (112, 54));
    let rect = layer.rect(slot(MONSTER));
    assert_eq!((rect.left(), rect.right()), (112, 127));
    assert_eq!((rect.top(), rect.bottom()), (54, 68));
}

#[test]
fn set_pending_changes_nothing_visible() {
    let bg = background();
    let mut fb = bg.clone();
    let mut layer = SpriteLayer::new();
    let monster = slot(MONSTER);
    show(&mut layer, &mut fb, MONSTER, 69, (100, 50));
    let crc = fb.crc32();
    let rect = layer.rect(monster);
    layer.set_pending(monster, img(70));
    assert_eq!(fb.crc32(), crc);
    assert_eq!(layer.image(monster), img(69));
    assert_eq!(layer.pending_image(monster), img(70));
    assert_eq!(layer.rect(monster), rect);
    assert!(layer.collisions().is_empty());

    layer.move_to(&mut fb, monster, Px::new(100, 50));
    assert_eq!(layer.image(monster), img(70));
    let mut expected = bg.clone();
    expected.blit_masked(img(70), Px::new(100, 50));
    assert_eq!(fb, expected);
}

#[test]
fn reset_does_not_erase() {
    let bg = background();
    let mut fb = bg.clone();
    let mut layer = SpriteLayer::new();
    let digger = slot(DIGGER);
    show(&mut layer, &mut fb, DIGGER, DIGGER_IMAGE, (100, 60));
    let crc = fb.crc32();
    let saved = layer.saved_background(digger).cloned();
    layer.reset(digger, ImageId::BLANK);
    assert_eq!(fb.crc32(), crc, "the image stays on the picture");
    assert!(!layer.is_enabled(digger));
    assert_eq!(layer.position(digger), Px::new(100, 60));
    assert_eq!(layer.image(digger), ImageId::BLANK);
    assert_eq!(layer.pending_image(digger), ImageId::BLANK);
    assert_eq!(layer.saved_background(digger).cloned(), saved);

    // Another sprite moved onto the old rectangle neither lists, restores
    // nor redraws the forgotten one.
    let monster = slot(MONSTER);
    show(&mut layer, &mut fb, MONSTER, 69, (20, 60));
    let set = layer.redraw_set(
        Rect::of_image(Px::new(104, 60), img(69).size()),
        Some(monster),
    );
    assert!(!set.contains(digger));
    let before = fb.clone();
    let hits = layer.move_to(&mut fb, monster, Px::new(104, 60));
    assert!(hits.is_empty());
    let mut expected = before;
    expected.restore(
        Px::new(20, 60),
        &bg.capture(Px::new(20, 60), img(69).size()),
    );
    expected.blit_masked(img(69), Px::new(104, 60));
    assert_eq!(fb, expected);
}

#[test]
fn overlap_16x15_pairs() {
    let mut fb = background();
    let mut layer = SpriteLayer::new();
    let (a, b) = (slot(MONSTER), slot(DIGGER));
    for dx in (-28..=28).step_by(4) {
        for dy in -20..=20 {
            show(&mut layer, &mut fb, MONSTER, 69, (100, 60));
            layer.reset(b, img(DIGGER_IMAGE));
            layer.place(&mut fb, b, Px::new(100 + dx, 60 + dy));
            layer.reset(a, img(69));
            let expected = dx.abs() <= 12 && dy.abs() <= 14;
            assert_eq!(layer.overlaps(a, b), expected, "dx {dx} dy {dy}");
            assert_eq!(layer.overlaps(b, a), expected, "dx {dx} dy {dy}");
            layer.reset(b, img(DIGGER_IMAGE));
        }
    }
}

#[test]
fn overlap_touching_edges() {
    let mut fb = background();
    let mut layer = SpriteLayer::new();
    let (a, b) = (slot(BAG), slot(MONSTER));
    show(&mut layer, &mut fb, BAG, 62, (100, 60));
    for (x, y, expected) in [
        (116, 60, false),
        (112, 60, true),
        (84, 60, false),
        (88, 60, true),
        (100, 75, false),
        (100, 74, true),
        (100, 45, false),
        (100, 46, true),
    ] {
        layer.reset(b, img(69));
        layer.place(&mut fb, b, Px::new(x, y));
        assert_eq!(layer.overlaps(a, b), expected, "({x}, {y})");
    }
}

#[test]
fn overlap_fireball_vs_16x15() {
    let mut fb = background();
    let mut layer = SpriteLayer::new();
    let (fireball, digger) = (slot(14), slot(DIGGER));
    let (sx, sy) = (100, 60);
    show(&mut layer, &mut fb, DIGGER, DIGGER_IMAGE, (sx, sy));
    assert_eq!(img(82).size().w(), 2);
    assert_eq!(img(82).size().h(), 8);
    for fx in (sx - 24..=sx + 24).step_by(4) {
        for fy in sy - 16..=sy + 24 {
            layer.reset(fireball, img(82));
            layer.place(&mut fb, fireball, Px::new(fx, fy));
            let expected = (-4..=12).contains(&(fx - sx)) && (sy - 7..=sy + 14).contains(&fy);
            assert_eq!(layer.overlaps(fireball, digger), expected, "({fx}, {fy})");
        }
    }
}

// 7.3 Redraw sets.

/// A at (100, 60) overlaps B at (112, 60), which overlaps C at (124, 60);
/// A and C do not overlap.
fn chain() -> (Framebuffer, SpriteLayer, [SpriteId; 3]) {
    let mut fb = background();
    let mut layer = SpriteLayer::new();
    show(&mut layer, &mut fb, 1, 62, (100, 60));
    show(&mut layer, &mut fb, 2, 62, (112, 60));
    show(&mut layer, &mut fb, 3, 62, (124, 60));
    let ids = [slot(1), slot(2), slot(3)];
    assert!(layer.overlaps(ids[0], ids[1]) && layer.overlaps(ids[1], ids[2]));
    assert!(!layer.overlaps(ids[0], ids[2]));
    (fb, layer, ids)
}

#[test]
fn redraw_set_chain() {
    let (_, layer, [a, b, c]) = chain();
    let set = layer.redraw_set(layer.rect(a), Some(a));
    assert!(set.contains(b) && set.contains(c));
    let set = layer.redraw_set(layer.rect(c), Some(c));
    assert!(set.contains(a) && set.contains(b));
}

#[test]
fn redraw_set_skips_disabled() {
    let (mut fb, mut layer, [a, b, c]) = chain();
    layer.erase(&mut fb, b);
    let set = layer.redraw_set(layer.rect(a), Some(a));
    assert!(!set.contains(b) && !set.contains(c));
    assert!(set.is_empty());
}

#[test]
fn redraw_set_isolated_is_empty() {
    let (mut fb, mut layer, _) = chain();
    show(&mut layer, &mut fb, MONSTER, 69, (200, 120));
    assert!(
        layer
            .redraw_set(layer.rect(slot(MONSTER)), Some(slot(MONSTER)))
            .is_empty()
    );
    // An area overlapping nothing, with no starting slot.
    assert!(
        layer
            .redraw_set(Rect::new(Px::new(0, 150), 16, 15), None)
            .is_empty()
    );
}

#[test]
fn redraw_set_includes_enabled_start_slot() {
    let (_, mut layer, [a, b, c]) = chain();
    let set = layer.redraw_set(layer.rect(a), Some(a));
    assert_eq!(numbers(set.iter()), [a, b, c].map(SpriteId::number));
    // Disabled, the starting slot only supplies the rectangle.
    layer.reset(a, img(62));
    let set = layer.redraw_set(layer.rect(a), Some(a));
    assert!(!set.contains(a));
    assert!(set.contains(b) && set.contains(c));
    // With no starting slot the rectangle alone is used.
    let set = layer.redraw_set(Rect::new(Px::new(100, 60), 4, 1), None);
    assert!(!set.contains(a) && !set.contains(b));
}

// 7.4 Placing.

#[test]
fn place_first_appearance() {
    let bg = background();
    let mut fb = bg.clone();
    let mut layer = SpriteLayer::new();
    show(&mut layer, &mut fb, BAG, 62, (100, 50));
    let bag = slot(BAG);
    assert!(layer.is_enabled(bag));
    assert_eq!(fb, composite(&bg, &layer));
    assert_ne!(fb, bg);
    assert_eq!(
        layer.saved_background(bag),
        Some(&bg.capture(Px::new(100, 50), img(62).size()))
    );
}

#[test]
fn place_under_higher_sprite_saves_clean_background() {
    let bg = background();
    let mut fb = bg.clone();
    let mut layer = SpriteLayer::new();
    show(&mut layer, &mut fb, DIGGER, DIGGER_IMAGE, (100, 50));
    show(&mut layer, &mut fb, BAG, 62, (108, 52));
    assert!(layer.overlaps(slot(BAG), slot(DIGGER)));
    assert_eq!(
        layer.saved_background(slot(BAG)),
        Some(&bg.capture(Px::new(108, 52), img(62).size()))
    );
    assert_eq!(
        fb,
        composite(&bg, &layer),
        "the digger is drawn over the bag"
    );
}

#[test]
fn place_does_not_touch_collisions() {
    let mut fb = background();
    let mut layer = SpriteLayer::new();
    show(&mut layer, &mut fb, BAG, 62, (100, 60));
    show(&mut layer, &mut fb, DIGGER, DIGGER_IMAGE, (40, 60));
    let hits = layer.move_to(&mut fb, slot(DIGGER), Px::new(104, 60));
    assert_eq!(list(hits, SpriteClass::Bag), [1]);
    show(&mut layer, &mut fb, MONSTER, 69, (108, 62));
    assert_eq!(layer.collisions(), hits);
    show(&mut layer, &mut fb, 9, 69, (200, 100));
    assert_eq!(layer.collisions(), hits);
}

#[test]
fn place_enabled_sprite_quirk() {
    let bg = background();
    let monster = slot(MONSTER);

    // The old image stays when an enabled sprite is placed elsewhere.
    let mut fb = bg.clone();
    let mut layer = SpriteLayer::new();
    show(&mut layer, &mut fb, MONSTER, 69, (100, 50));
    layer.place(&mut fb, monster, Px::new(200, 50));
    assert_eq!(layer.position(monster), Px::new(200, 50));
    let mut expected = bg.clone();
    expected.blit_masked(img(69), Px::new(100, 50));
    expected.blit_masked(img(69), Px::new(200, 50));
    assert_eq!(fb, expected);

    // Placed onto another enabled sprite, its old background is pasted at
    // the new position before the new capture.
    let mut fb = bg.clone();
    let mut layer = SpriteLayer::new();
    show(&mut layer, &mut fb, MONSTER, 69, (100, 50));
    let stale = layer
        .saved_background(monster)
        .cloned()
        .unwrap_or_else(|| unreachable!("the monster was placed"));
    show(&mut layer, &mut fb, BAG, 62, (200, 50));
    assert_ne!(stale, bg.capture(Px::new(204, 50), img(69).size()));
    layer.place(&mut fb, monster, Px::new(204, 50));
    let mut expected = bg.clone();
    expected.blit_masked(img(69), Px::new(100, 50));
    expected.restore(Px::new(204, 50), &stale);
    expected.blit_masked(img(62), Px::new(200, 50));
    expected.blit_masked(img(69), Px::new(204, 50));
    assert_eq!(fb, expected);
    assert_eq!(layer.saved_background(monster), Some(&stale));
}

#[test]
fn place_aligns_x() {
    let bg = background();
    let mut fb = bg.clone();
    let mut layer = SpriteLayer::new();
    show(&mut layer, &mut fb, BAG, 62, (115, 50));
    assert_eq!(layer.position(slot(BAG)), Px::new(112, 50));
    let mut expected = bg.clone();
    expected.blit_masked(img(62), Px::new(112, 50));
    assert_eq!(fb, expected);
}

// 7.5 Erasing.

#[test]
fn erase_lone_sprite_restores_picture() {
    let bg = background();
    let mut fb = bg.clone();
    let mut layer = SpriteLayer::new();
    show(&mut layer, &mut fb, BAG, 62, (100, 50));
    layer.erase(&mut fb, slot(BAG));
    assert_eq!(fb, bg);
    assert!(!layer.is_enabled(slot(BAG)));
    assert!(layer.saved_background(slot(BAG)).is_some());
}

#[test]
fn erase_under_overlapping_sprite() {
    let bg = background();
    let mut fb = bg.clone();
    let mut layer = SpriteLayer::new();
    show(&mut layer, &mut fb, BAG, 62, (100, 50));
    show(&mut layer, &mut fb, MONSTER, 69, (108, 53));
    layer.erase(&mut fb, slot(BAG));
    let mut expected = bg.clone();
    expected.blit_masked(img(69), Px::new(108, 53));
    assert_eq!(fb, expected);
    assert_eq!(fb, composite(&bg, &layer));
}

#[test]
fn erase_twice_is_noop() {
    let mut fb = background();
    let mut layer = SpriteLayer::new();
    show(&mut layer, &mut fb, BAG, 62, (100, 50));
    show(&mut layer, &mut fb, MONSTER, 69, (108, 53));
    layer.erase(&mut fb, slot(BAG));
    let (fb_before, layer_before) = (fb.clone(), layer.clone());
    layer.erase(&mut fb, slot(BAG));
    assert_eq!(fb, fb_before);
    assert_eq!(layer, layer_before);
    // A sprite that was never placed is disabled too.
    layer.erase(&mut fb, slot(DIGGER));
    assert_eq!(fb, fb_before);
    assert_eq!(layer, layer_before);
}

#[test]
fn erase_keeps_collisions() {
    let mut fb = background();
    let mut layer = SpriteLayer::new();
    show(&mut layer, &mut fb, BAG, 62, (100, 60));
    show(&mut layer, &mut fb, DIGGER, DIGGER_IMAGE, (40, 60));
    let hits = layer.move_to(&mut fb, slot(DIGGER), Px::new(104, 60));
    assert_eq!(list(hits, SpriteClass::Bag), [1]);
    layer.erase(&mut fb, slot(BAG));
    assert_eq!(layer.collisions(), hits, "the lists still name the bag");
    assert!(layer.saved_background(slot(BAG)).is_some());
}

// 7.6 Moving.

#[test]
fn move_removes_old_image() {
    let bg = background();
    let mut fb = bg.clone();
    let mut layer = SpriteLayer::new();
    show(&mut layer, &mut fb, MONSTER, 69, (100, 50));
    layer.move_to(&mut fb, slot(MONSTER), Px::new(104, 50));
    let mut expected = bg.clone();
    expected.blit_masked(img(69), Px::new(104, 50));
    assert_eq!(fb, expected);
    let old = Rect::new(Px::new(100, 50), 4, 15);
    assert_eq!(region(&fb, old), region(&bg, old));
}

#[test]
fn move_repairs_old_neighbour() {
    let bg = background();
    // The moved sprite above and below its neighbour in stacking order.
    for (moved, neighbour) in [(DIGGER, MONSTER), (BAG, MONSTER)] {
        let mut fb = bg.clone();
        let mut layer = SpriteLayer::new();
        show(&mut layer, &mut fb, moved, DIGGER_IMAGE, (100, 50));
        show(&mut layer, &mut fb, neighbour, 69, (108, 52));
        layer.move_to(&mut fb, slot(moved), Px::new(160, 50));
        assert!(!layer.overlaps(slot(moved), slot(neighbour)));
        let mut expected = bg.clone();
        expected.blit_masked(img(69), Px::new(108, 52));
        expected.blit_masked(img(DIGGER_IMAGE), Px::new(160, 50));
        assert_eq!(fb, expected, "moved slot {moved}");
    }
}

#[test]
fn move_onto_neighbour_saves_clean_background() {
    let bg = background();
    for moved in [DIGGER, BAG] {
        let mut fb = bg.clone();
        let mut layer = SpriteLayer::new();
        show(&mut layer, &mut fb, MONSTER, 69, (100, 50));
        show(&mut layer, &mut fb, moved, DIGGER_IMAGE, (40, 50));
        layer.move_to(&mut fb, slot(moved), Px::new(104, 52));
        assert_eq!(
            layer.saved_background(slot(moved)),
            Some(&bg.capture(Px::new(104, 52), img(DIGGER_IMAGE).size())),
            "moved slot {moved}"
        );
        assert_eq!(fb, composite(&bg, &layer), "moved slot {moved}");
    }
}

#[test]
fn move_in_place_animates() {
    let bg = background();
    let mut fb = bg.clone();
    let mut layer = SpriteLayer::new();
    let digger = slot(DIGGER);
    show(&mut layer, &mut fb, DIGGER, 1, (100, 50));
    layer.set_pending(digger, img(2));
    layer.move_to(&mut fb, digger, Px::new(100, 50));
    let mut expected = bg.clone();
    expected.blit_masked(img(2), Px::new(100, 50));
    assert_eq!(fb, expected);
    let mut overdrawn = bg.clone();
    overdrawn.blit_masked(img(1), Px::new(100, 50));
    overdrawn.blit_masked(img(2), Px::new(100, 50));
    assert_ne!(fb, overdrawn, "no pixel of the old frame remains");
}

#[test]
fn move_without_place_is_safe() {
    let bg = background();
    let mut fb = bg.clone();
    let mut layer = SpriteLayer::new();
    let digger = slot(DIGGER);
    layer.reset(digger, img(DIGGER_IMAGE));
    layer.move_to(&mut fb, digger, Px::new(100, 50));
    assert!(layer.is_enabled(digger));
    assert_eq!(layer.position(digger), Px::new(100, 50));
    let mut expected = bg.clone();
    expected.blit_masked(img(DIGGER_IMAGE), Px::new(100, 50));
    assert_eq!(fb, expected);
}

#[test]
fn move_after_erase_restores_stale_background() {
    let bg = background();
    let mut fb = bg.clone();
    let mut layer = SpriteLayer::new();
    let monster = slot(MONSTER);
    show(&mut layer, &mut fb, MONSTER, 69, (100, 50));
    let stale = layer
        .saved_background(monster)
        .cloned()
        .unwrap_or_else(|| unreachable!("the monster was placed"));
    layer.erase(&mut fb, monster);
    // The background changes after the erase.
    layer.draw_static_unprotected(&mut fb, img(98), Px::new(100, 50));
    let changed = fb.clone();
    layer.move_to(&mut fb, monster, Px::new(120, 50));
    let mut expected = changed.clone();
    expected.restore(Px::new(100, 50), &stale);
    expected.blit_masked(img(69), Px::new(120, 50));
    assert_eq!(fb, expected);
    let old = Rect::new(Px::new(100, 50), 16, 4);
    assert_ne!(
        region(&fb, old),
        region(&changed, old),
        "stale pixels pasted"
    );
}

#[test]
fn move_restores_saved_size() {
    let bg = background();
    let mut fb = bg.clone();
    let mut layer = SpriteLayer::new();
    let monster = slot(MONSTER);
    show(&mut layer, &mut fb, MONSTER, 69, (100, 50));
    let last_row = Rect::new(Px::new(100, 64), 16, 1);
    assert_ne!(region(&fb, last_row), region(&bg, last_row));
    layer.set_pending(monster, ImageId::HOBBIN_LEFT_DEAD);
    layer.move_to(&mut fb, monster, Px::new(200, 50));
    let mut expected = bg.clone();
    expected.blit_masked(ImageId::HOBBIN_LEFT_DEAD, Px::new(200, 50));
    assert_eq!(fb, expected, "all 15 rows of the old image are restored");
    assert_eq!(
        layer.saved_background(monster).map(SavedRect::size),
        Some(ImageId::HOBBIN_LEFT_DEAD.size())
    );
}

// 7.7 Collision lists.

#[test]
fn collisions_one_of_each() {
    let mut fb = background();
    let mut layer = SpriteLayer::new();
    show(&mut layer, &mut fb, 3, 62, (100, 60));
    show(&mut layer, &mut fb, 9, 69, (116, 60));
    show(&mut layer, &mut fb, DIGGER, DIGGER_IMAGE, (40, 60));
    show(&mut layer, &mut fb, 0, 81, (200, 120));
    let hits = layer.move_to(&mut fb, slot(DIGGER), Px::new(108, 60));
    assert_eq!(list(hits, SpriteClass::Bag), [3]);
    assert_eq!(list(hits, SpriteClass::Monster), [9]);
    for class in [
        SpriteClass::Bonus,
        SpriteClass::Fireball,
        SpriteClass::Digger,
    ] {
        assert!(list(hits, class).is_empty(), "{class:?}");
    }
    assert_eq!(layer.collisions(), hits);
}

#[test]
fn collisions_ascending_within_class() {
    let mut fb = background();
    let mut layer = SpriteLayer::new();
    show(&mut layer, &mut fb, 12, 69, (100, 60));
    show(&mut layer, &mut fb, 8, 69, (108, 60));
    show(&mut layer, &mut fb, 10, 69, (116, 60));
    show(&mut layer, &mut fb, 14, 82, (40, 60));
    let hits = layer.move_to(&mut fb, slot(14), Px::new(112, 60));
    assert_eq!(list(hits, SpriteClass::Monster), [8, 10, 12]);
    assert_eq!(hits.first(SpriteClass::Monster), Some(slot(8)));
}

#[test]
fn collisions_exclude_moved_sprite() {
    let mut fb = background();
    let mut layer = SpriteLayer::new();
    show(&mut layer, &mut fb, 16, DIGGER_IMAGE, (40, 60));
    show(&mut layer, &mut fb, 17, 32, (100, 60));
    let hits = layer.move_to(&mut fb, slot(16), Px::new(104, 60));
    assert_eq!(list(hits, SpriteClass::Digger), [17]);
    assert!(!hits.contains(slot(16)));
}

#[test]
fn collisions_exclude_disabled() {
    let mut fb = background();
    let mut layer = SpriteLayer::new();
    show(&mut layer, &mut fb, BAG, 62, (100, 60));
    layer.erase(&mut fb, slot(BAG));
    show(&mut layer, &mut fb, DIGGER, DIGGER_IMAGE, (40, 60));
    let hits = layer.move_to(&mut fb, slot(DIGGER), Px::new(100, 60));
    assert!(hits.is_empty());
}

#[test]
fn collisions_only_on_move() {
    let mut fb = background();
    let mut layer = SpriteLayer::new();
    let (bag, monster, digger) = (slot(BAG), slot(MONSTER), slot(DIGGER));
    show(&mut layer, &mut fb, BAG, 62, (100, 60));
    show(&mut layer, &mut fb, DIGGER, DIGGER_IMAGE, (40, 60));
    show(&mut layer, &mut fb, MONSTER, 69, (200, 120));

    // Overwritten by the next move.
    let hits = layer.move_to(&mut fb, digger, Px::new(104, 60));
    assert_eq!(list(hits, SpriteClass::Bag), [1]);
    assert!(
        layer
            .move_to(&mut fb, monster, Px::new(204, 120))
            .is_empty()
    );
    assert!(layer.collisions().is_empty());

    // Rectangles at the instant of the move.
    let hits = layer.move_to(&mut fb, monster, Px::new(108, 62));
    assert_eq!(list(hits, SpriteClass::Digger), [16]);
    assert_eq!(list(hits, SpriteClass::Bag), [1]);
    let hits = layer.move_to(&mut fb, digger, Px::new(40, 60));
    assert!(hits.is_empty());

    // Nothing but a move changes the stored lists.
    let hits = layer.move_to(&mut fb, digger, Px::new(104, 60));
    assert!(!hits.is_empty());
    layer.place(&mut fb, slot(9), Px::new(104, 62));
    layer.erase(&mut fb, bag);
    layer.reset(monster, img(70));
    layer.set_pending(digger, img(2));
    layer.draw_static(&mut fb, ImageId::TUNNEL_RIGHT, Px::new(104, 60));
    layer.draw_static_unprotected(&mut fb, ImageId::TUNNEL_UP, Px::new(40, 100));
    assert_eq!(layer.collisions(), hits);
}

#[test]
fn collision_uses_current_size() {
    let mut fb = background();
    let mut layer = SpriteLayer::new();
    let (monster, digger) = (slot(MONSTER), slot(DIGGER));
    show(&mut layer, &mut fb, MONSTER, 80, (100, 60));
    layer.set_pending(monster, img(77));
    show(&mut layer, &mut fb, DIGGER, DIGGER_IMAGE, (40, 80));
    let hits = layer.move_to(&mut fb, digger, Px::new(100, 74));
    assert!(hits.is_empty(), "the 14-row rectangle is used");

    // The same arrangement with a 15-row current image collides.
    layer.move_to(&mut fb, digger, Px::new(40, 80));
    layer.move_to(&mut fb, monster, Px::new(100, 60));
    assert_eq!(layer.image(monster), img(77));
    let hits = layer.move_to(&mut fb, digger, Px::new(100, 74));
    assert_eq!(list(hits, SpriteClass::Monster), [8]);
}

#[test]
fn enabled_not_alive() {
    let mut fb = background();
    let mut layer = SpriteLayer::new();
    let (monster, digger) = (slot(MONSTER), slot(DIGGER));
    // A gravestone is still an enabled sprite.
    show(&mut layer, &mut fb, DIGGER, 26, (100, 60));
    show(&mut layer, &mut fb, MONSTER, 69, (40, 60));
    let hits = layer.move_to(&mut fb, monster, Px::new(104, 60));
    assert_eq!(list(hits, SpriteClass::Digger), [16]);

    // A blinking digger, erased for a frame, is not.
    layer.move_to(&mut fb, monster, Px::new(40, 60));
    layer.erase(&mut fb, digger);
    let hits = layer.move_to(&mut fb, monster, Px::new(104, 60));
    assert_eq!(list(hits, SpriteClass::Digger), [0u8; 0]);
}

#[test]
fn clear_collisions_empties_lists() {
    let mut fb = background();
    let mut layer = SpriteLayer::new();
    show(&mut layer, &mut fb, BAG, 62, (100, 60));
    show(&mut layer, &mut fb, DIGGER, DIGGER_IMAGE, (40, 60));
    let hits = layer.move_to(&mut fb, slot(DIGGER), Px::new(104, 60));
    assert!(!hits.is_empty());
    let crc = fb.crc32();
    layer.clear_collisions();
    assert!(layer.collisions().is_empty());
    for class in SpriteClass::ALL {
        assert_eq!(layer.collisions().first(class), None);
    }
    assert_eq!(fb.crc32(), crc);
}

// 7.8 Static drawing.

#[test]
fn static_draw_under_digger_becomes_background() {
    let bg = background();
    let mut fb = bg.clone();
    let mut layer = SpriteLayer::new();
    let digger = slot(DIGGER);
    show(&mut layer, &mut fb, DIGGER, DIGGER_IMAGE, (100, 60));
    layer.draw_static(&mut fb, ImageId::TUNNEL_RIGHT, Px::new(104, 58));
    let mut carved = bg.clone();
    carved.blit_masked(ImageId::TUNNEL_RIGHT, Px::new(104, 58));
    assert_ne!(carved, bg);
    assert_eq!(fb, composite(&carved, &layer), "the digger stays on top");
    assert_eq!(
        layer.saved_background(digger),
        Some(&carved.capture(Px::new(100, 60), img(DIGGER_IMAGE).size()))
    );
    layer.erase(&mut fb, digger);
    assert_eq!(fb, carved, "the carved shape shows where the digger stood");
}

#[test]
fn static_draw_without_sprites_equals_direct_blit() {
    let bg = background();
    let mut fb = bg.clone();
    let mut layer = SpriteLayer::new();
    show(&mut layer, &mut fb, DIGGER, DIGGER_IMAGE, (200, 120));
    let mut expected = fb.clone();
    let layer_before = layer.clone();
    layer.draw_static(&mut fb, ImageId::TUNNEL_UP, Px::new(100, 60));
    expected.blit_masked(ImageId::TUNNEL_UP, Px::new(100, 60));
    assert_eq!(fb, expected);
    assert_eq!(layer, layer_before);
}

#[test]
fn static_draw_leaves_distant_sprites() {
    let bg = background();
    let mut fb = bg.clone();
    let mut layer = SpriteLayer::new();
    let far = slot(MONSTER);
    show(&mut layer, &mut fb, DIGGER, DIGGER_IMAGE, (100, 60));
    show(&mut layer, &mut fb, MONSTER, 69, (200, 120));
    // Scribble over the distant sprite behind the layer's back: if it were
    // restored or redrawn, the scribble would disappear.
    layer.draw_static_unprotected(&mut fb, img(98), Px::new(200, 120));
    let scribbled = fb.clone();
    let saved = layer.saved_background(far).cloned();
    layer.draw_static(&mut fb, ImageId::TUNNEL_RIGHT, Px::new(104, 58));
    let area = layer.rect(far);
    assert_eq!(region(&fb, area), region(&scribbled, area));
    assert_eq!(layer.saved_background(far).cloned(), saved);
}

#[test]
fn static_unprotected_touches_no_sprite() {
    let mut fb = Framebuffer::new();
    let mut layer = SpriteLayer::new();
    show(&mut layer, &mut fb, DIGGER, DIGGER_IMAGE, (24, 12));
    let (before, layer_before) = (fb.clone(), layer.clone());
    layer.draw_static_unprotected(&mut fb, img(94), Px::new(20, 14));
    assert_eq!(layer, layer_before);
    let changed: Vec<(usize, usize)> = before
        .pixels()
        .iter()
        .zip(fb.pixels().iter())
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, _)| (i % W, i / W))
        .collect();
    assert_ne!(changed.len(), 0);
    assert!(
        changed
            .iter()
            .all(|&(x, y)| (40..80).contains(&x) && (28..36).contains(&y))
    );
}

#[test]
fn alignment_sites() {
    let bg = background();
    let mut fb = bg.clone();
    let mut layer = SpriteLayer::new();

    // Place and move align x; y is never altered.
    show(&mut layer, &mut fb, BAG, 62, (115, 50));
    assert_eq!(layer.position(slot(BAG)), Px::new(112, 50));
    layer.move_to(&mut fb, slot(BAG), Px::new(-3, 50));
    assert_eq!(layer.position(slot(BAG)), Px::new(-4, 50));
    layer.move_to(&mut fb, slot(BAG), Px::new(292, 181));
    assert_eq!(layer.position(slot(BAG)), Px::new(292, 181));

    // Both static draws draw at the aligned x.
    for protected in [true, false] {
        let mut fb = bg.clone();
        let mut expected = bg.clone();
        if protected {
            layer.draw_static(&mut fb, ImageId::TUNNEL_UP, Px::new(82, 100));
        } else {
            layer.draw_static_unprotected(&mut fb, ImageId::TUNNEL_UP, Px::new(82, 100));
        }
        expected.blit_masked(ImageId::TUNNEL_UP, Px::new(80, 100));
        assert_eq!(fb, expected, "protected {protected}");
    }

    // The protected area uses x as given: an 8-column image at x 82 covers
    // columns 82–89, so a sprite at x 88 is protected, although the image
    // itself, drawn at 80, ends at column 87.
    for (x, protected) in [(82, true), (80, false)] {
        let mut fb = bg.clone();
        let mut layer = SpriteLayer::new();
        show(&mut layer, &mut fb, MONSTER, 69, (88, 100));
        let clean = region(&fb, layer.rect(slot(MONSTER)));
        layer.draw_static_unprotected(&mut fb, img(98), Px::new(88, 100));
        let scribbled = region(&fb, layer.rect(slot(MONSTER)));
        assert_ne!(clean, scribbled);
        layer.draw_static(&mut fb, ImageId::TUNNEL_RIGHT, Px::new(x, 100));
        let after = region(&fb, layer.rect(slot(MONSTER)));
        if protected {
            assert_eq!(after, clean, "x {x}: the sprite was restored and redrawn");
        } else {
            assert_eq!(after, scribbled, "x {x}: the sprite was left alone");
        }
    }
}

// 7.9 Stacking order.

fn ascending_and_descending(bg: &Framebuffer, layer: &SpriteLayer) -> (Framebuffer, Framebuffer) {
    let mut order: Vec<SpriteId> = SpriteId::all().collect();
    let ascending = composite_in(bg, layer, &order);
    order.reverse();
    let descending = composite_in(bg, layer, &order);
    (ascending, descending)
}

#[test]
fn stacking_order() {
    let bg = background();
    let mut fb = bg.clone();
    let mut layer = SpriteLayer::new();
    show(&mut layer, &mut fb, BAG, 62, (100, 60));
    show(&mut layer, &mut fb, DIGGER, DIGGER_IMAGE, (40, 60));
    layer.move_to(&mut fb, slot(DIGGER), Px::new(104, 62));
    let (ascending, descending) = ascending_and_descending(&bg, &layer);
    assert_ne!(ascending, descending, "the overlap is visible");
    assert_eq!(fb, ascending, "digger over bag after moving the digger");
    layer.move_to(&mut fb, slot(BAG), Px::new(100, 61));
    let (ascending, _) = ascending_and_descending(&bg, &layer);
    assert_eq!(fb, ascending, "digger over bag after moving the bag");
}

#[test]
fn stacking_order_within_class() {
    let bg = background();
    let mut fb = bg.clone();
    let mut layer = SpriteLayer::new();
    show(&mut layer, &mut fb, 9, 69, (100, 60));
    show(&mut layer, &mut fb, 8, 73, (40, 60));
    layer.move_to(&mut fb, slot(8), Px::new(104, 63));
    let (ascending, descending) = ascending_and_descending(&bg, &layer);
    assert_ne!(ascending, descending, "the overlap is visible");
    assert_eq!(fb, ascending, "slot 9 over slot 8, moved last");
    layer.move_to(&mut fb, slot(9), Px::new(100, 61));
    let (ascending, _) = ascending_and_descending(&bg, &layer);
    assert_eq!(fb, ascending, "slot 9 over slot 8, moved last");
}
