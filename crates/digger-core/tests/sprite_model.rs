//! Model-based tests of the sprite layer (design D8).
//!
//! A fixed-seed generator produces long sequences of operations that follow
//! the original game's call discipline: a disabled sprite is placed before it
//! is moved, an enabled sprite is never placed, and only disabled sprites are
//! reset. After every step two oracles that share no code with the sprite
//! layer are checked:
//!
//! - the picture equals the static background (the tiled level background
//!   with every protected static draw applied directly) with each enabled
//!   sprite's current image drawn over it in ascending slot order;
//! - the collision lists equal a brute-force rectangle scan done after the
//!   most recent move.
//!
//! At the end every sprite is erased and the picture must equal the static
//! background exactly (sprite-layer spec, "Saved backgrounds hold clean
//! background").

use digger_core::assets::ImageId;
use digger_core::geom::Px;
use digger_core::sprite::{SpriteClass, SpriteId, SpriteLayer};
use digger_core::video::{Framebuffer, LevelPlan};

/// Number of independent sequences.
const SEQUENCES: u64 = 24;
/// Operations per sequence.
const STEPS: usize = 2000;
/// Sprite slots.
const SLOTS: usize = 18;

/// The `SplitMix64` generator: small, fixed, and written here so the test needs no
/// dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A value in `0..n`; `n` is small and non-zero.
    fn below(&mut self, n: u32) -> u32 {
        u32::try_from(self.next() % u64::from(n)).unwrap_or(0)
    }

    /// A value in `lo..=hi`.
    fn range(&mut self, lo: i32, hi: i32) -> i32 {
        let span = u32::try_from(hi - lo + 1).unwrap_or(1);
        lo + i32::try_from(self.below(span)).unwrap_or(0)
    }

    fn image(&mut self) -> ImageId {
        let n = u8::try_from(self.below(120)).unwrap_or(0);
        ImageId::new(n).unwrap_or(ImageId::BLANK)
    }

    fn slot(&mut self) -> SpriteId {
        let n = u8::try_from(self.below(18)).unwrap_or(0);
        SpriteId::new(n).unwrap_or(SpriteId::BONUS)
    }
}

/// The area positions are drawn from: dense arenas give long overlap chains,
/// the full arena exercises clipping at all four edges.
#[derive(Clone, Copy)]
struct Arena {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl Arena {
    fn for_sequence(seq: u64) -> Self {
        match seq % 3 {
            0 => Self {
                left: 60,
                top: 40,
                right: 130,
                bottom: 90,
            },
            1 => Self {
                left: -24,
                top: -20,
                right: 330,
                bottom: 210,
            },
            _ => Self {
                left: -24,
                top: 150,
                right: 90,
                bottom: 210,
            },
        }
    }

    fn point(self, rng: &mut Rng) -> Px {
        Px::new(
            rng.range(self.left, self.right),
            rng.range(self.top, self.bottom),
        )
    }
}

/// What the oracle knows about one sprite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ModelSprite {
    enabled: bool,
    x: i32,
    y: i32,
    image: ImageId,
    pending: ImageId,
}

/// The independent model: sprite state, the static background, and the
/// collision lists of the last move as plain slot numbers.
struct Model {
    sprites: [ModelSprite; SLOTS],
    background: Framebuffer,
    lists: Vec<u8>,
}

/// Rounds down to a multiple of 4, towards minus infinity.
fn floor4(x: i32) -> i32 {
    x.div_euclid(4) * 4
}

/// The inclusive logical rectangle (left, top, right, bottom) of an image at
/// a position.
fn rect_of(x: i32, y: i32, image: ImageId) -> (i32, i32, i32, i32) {
    let size = image.size();
    let w = 4 * i32::from(size.w());
    let h = i32::from(size.h());
    (x, y, x + w - 1, y + h - 1)
}

fn rects_overlap(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> bool {
    a.0 <= b.2 && b.0 <= a.2 && a.1 <= b.3 && b.1 <= a.3
}

impl Model {
    fn sprite(&self, n: usize) -> ModelSprite {
        self.sprites.get(n).copied().unwrap_or_else(|| {
            unreachable!("slot {n} is below 18");
        })
    }

    fn sprite_mut(&mut self, n: usize) -> &mut ModelSprite {
        self.sprites
            .get_mut(n)
            .unwrap_or_else(|| unreachable!("slot {n} is below 18"))
    }

    /// The static background with the enabled sprites drawn over it in
    /// ascending slot order.
    fn composite(&self) -> Framebuffer {
        let mut fb = self.background.clone();
        for sprite in self.sprites.iter().filter(|s| s.enabled) {
            fb.blit_masked(sprite.image, Px::new(sprite.x, sprite.y));
        }
        fb
    }

    /// The brute-force collision scan for a move of slot `moved`: every
    /// enabled other slot whose rectangle overlaps, ascending.
    fn scan(&self, moved: usize) -> Vec<u8> {
        let m = self.sprite(moved);
        let area = rect_of(m.x, m.y, m.image);
        (0..SLOTS)
            .filter(|&n| n != moved)
            .filter(|&n| {
                let s = self.sprite(n);
                s.enabled && rects_overlap(rect_of(s.x, s.y, s.image), area)
            })
            .filter_map(|n| u8::try_from(n).ok())
            .collect()
    }
}

fn index(id: SpriteId) -> usize {
    usize::from(id.number())
}

/// The layer's stored lists, class by class, each ascending.
fn layer_lists(layer: &SpriteLayer) -> Vec<u8> {
    SpriteClass::ALL
        .iter()
        .flat_map(|&class| layer.collisions().of(class).map(SpriteId::number))
        .collect()
}

/// Checks both oracles and the per-sprite state after one step.
fn check(layer: &SpriteLayer, fb: &Framebuffer, model: &Model, seq: u64, step: usize, op: &str) {
    for id in SpriteId::all() {
        let s = model.sprite(index(id));
        assert_eq!(
            layer.is_enabled(id),
            s.enabled,
            "seq {seq} step {step} ({op}): enabled flag of slot {}",
            id.number()
        );
        assert_eq!(
            layer.image(id),
            s.image,
            "seq {seq} step {step} ({op}): current image of slot {}",
            id.number()
        );
        assert_eq!(
            layer.pending_image(id),
            s.pending,
            "seq {seq} step {step} ({op}): pending image of slot {}",
            id.number()
        );
        if s.enabled {
            assert_eq!(
                layer.position(id),
                Px::new(s.x, s.y),
                "seq {seq} step {step} ({op}): position of slot {}",
                id.number()
            );
        }
    }
    assert_eq!(
        layer_lists(layer),
        model.lists,
        "seq {seq} step {step} ({op}): collision lists"
    );
    let expected = model.composite();
    assert!(
        fb.pixels() == expected.pixels(),
        "seq {seq} step {step} ({op}): picture differs from the composite \
         (crc {:08X}, expected {:08X})",
        fb.crc32(),
        expected.crc32()
    );
}

/// Runs one sequence and returns the CRC-32 of the final picture before the
/// final erase, so that determinism can be checked across runs.
fn run_sequence(seq: u64) -> u32 {
    let mut rng = Rng(0xD166_E500_0000_0000 ^ seq.wrapping_mul(0x0123_4567_89AB_CDEF));
    let arena = Arena::for_sequence(seq);
    let plan_number = u8::try_from(seq % 8).unwrap_or(0) + 1;
    let plan = LevelPlan::new(plan_number).unwrap_or_else(|| unreachable!("plan 1-8 exists"));

    let mut fb = Framebuffer::new();
    fb.draw_background(plan);
    let mut model = Model {
        sprites: [ModelSprite {
            enabled: false,
            x: 0,
            y: 0,
            image: ImageId::BLANK,
            pending: ImageId::BLANK,
        }; SLOTS],
        background: fb.clone(),
        lists: Vec::new(),
    };
    let mut layer = SpriteLayer::new();
    check(&layer, &fb, &model, seq, 0, "start");

    for step in 1..=STEPS {
        let id = rng.slot();
        let n = index(id);
        let s = model.sprite(n);
        let op = match rng.below(10) {
            // Place: only a disabled sprite, after a reset or a pending
            // change.
            0..=2 if !s.enabled => {
                let image = rng.image();
                if rng.below(2) == 0 {
                    layer.reset(id, image);
                    let m = model.sprite_mut(n);
                    m.image = image;
                    m.pending = image;
                } else {
                    layer.set_pending(id, image);
                    model.sprite_mut(n).pending = image;
                }
                let at = arena.point(&mut rng);
                layer.place(&mut fb, id, at);
                let m = model.sprite_mut(n);
                m.x = floor4(at.x);
                m.y = at.y;
                m.image = m.pending;
                m.enabled = true;
                "place"
            }
            // Move an enabled sprite, sometimes in place, sometimes with a
            // new pending image.
            0..=5 if s.enabled => {
                if rng.below(3) == 0 {
                    let image = rng.image();
                    layer.set_pending(id, image);
                    model.sprite_mut(n).pending = image;
                }
                let at = match rng.below(4) {
                    0 => Px::new(s.x, s.y),
                    1 => Px::new(s.x + rng.range(-8, 8), s.y + rng.range(-6, 6)),
                    _ => arena.point(&mut rng),
                };
                let returned = layer.move_to(&mut fb, id, at);
                let m = model.sprite_mut(n);
                m.x = floor4(at.x);
                m.y = at.y;
                m.image = m.pending;
                model.lists = model.scan(n);
                assert_eq!(
                    returned,
                    layer.collisions(),
                    "seq {seq} step {step}: returned and stored lists differ"
                );
                "move"
            }
            6 => {
                let image = rng.image();
                layer.set_pending(id, image);
                model.sprite_mut(n).pending = image;
                "set_pending"
            }
            7 => {
                // Erasing a disabled sprite is allowed and does nothing.
                layer.erase(&mut fb, id);
                model.sprite_mut(n).enabled = false;
                "erase"
            }
            _ => {
                // Protected static draw at an aligned x: the protected area
                // and the drawn image then coincide.
                let image = rng.image();
                let at = arena.point(&mut rng);
                let at = Px::new(floor4(at.x), at.y);
                layer.draw_static(&mut fb, image, at);
                model.background.blit_masked(image, at);
                "draw_static"
            }
        };
        check(&layer, &fb, &model, seq, step, op);
    }

    let final_crc = fb.crc32();
    for id in SpriteId::all() {
        layer.erase(&mut fb, id);
    }
    assert!(
        fb.pixels() == model.background.pixels(),
        "seq {seq}: erasing everything does not leave the static background"
    );
    final_crc
}

#[test]
fn disciplined_sequences_match_composite_and_scan() {
    for seq in 0..SEQUENCES {
        run_sequence(seq);
    }
}

#[test]
fn sequences_are_deterministic() {
    assert_eq!(run_sequence(5), run_sequence(5));
}
