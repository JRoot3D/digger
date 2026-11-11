//! Content guarantees over the embedded sprite data (spec "game-assets":
//! image and mask values, erasers and opaque blocks, tiles and tunnels).

use super::{Image, ImageId};

fn image(n: u8) -> Image<'static> {
    ImageId::new(n)
        .unwrap_or_else(|| unreachable!("test image number {n} is valid"))
        .get()
}

fn value_set(bytes: &[u8]) -> Vec<u8> {
    let mut seen = [false; 256];
    for &b in bytes {
        if let Some(slot) = seen.get_mut(usize::from(b)) {
            *slot = true;
        }
    }
    (0..=255)
        .filter(|&v| seen.get(usize::from(v)) == Some(&true))
        .collect()
}

#[test]
fn image_values_in_allowed_set() {
    for id in ImageId::all() {
        for &value in id.get().pixels() {
            assert!(
                matches!(value, 255 | 0 | 2 | 3 | 4 | 6 | 7 | 8 | 10 | 12 | 14 | 15),
                "image {} holds {value}",
                id.number()
            );
        }
    }
}

#[test]
fn mask_values_are_0_or_255() {
    for id in ImageId::all() {
        for &value in id.get().mask() {
            assert!(
                value == 0 || value == 255,
                "mask {} holds {value}",
                id.number()
            );
        }
    }
}

#[test]
fn coloured_pixels_have_zero_mask() {
    for id in ImageId::all() {
        let data = id.get();
        for (offset, (&pixel, &mask)) in data.pixels().iter().zip(data.mask()).enumerate() {
            if pixel != 0 && pixel != 255 {
                assert_eq!(mask, 0, "image {} byte {offset}", id.number());
            }
        }
    }
}

#[test]
fn eraser_images_are_zero() {
    for n in [0, 31, 102, 103, 104, 105, 106, 107, 109, 112] {
        assert!(image(n).pixels().iter().all(|&b| b == 0), "image {n}");
    }
    // The emerald eraser shares the emerald silhouette.
    assert_eq!(image(108).mask(), image(109).mask());
}

#[test]
fn opaque_images_have_zero_mask() {
    for n in [0, 31, 81].into_iter().chain(94..=101) {
        assert!(image(n).mask().iter().all(|&b| b == 0), "mask {n}");
    }
    assert!(!image(94).pixels().contains(&255));
    let blank = image(0);
    assert_eq!(blank.pixels().len(), 960);
    assert_eq!(blank.mask().len(), 960);
}

#[test]
fn tile_colour_sets() {
    let expected: [&[u8]; 8] = [
        &[4, 6],
        &[2, 4],
        &[4, 6],
        &[2, 6],
        &[2, 4, 6],
        &[2, 4],
        &[2, 4, 6],
        &[4, 6],
    ];
    for (plan, colours) in (1..=8).zip(expected) {
        let tile = ImageId::background(plan)
            .unwrap_or_else(|| unreachable!("plan {plan} is valid"))
            .get();
        assert_eq!(value_set(tile.pixels()), colours, "plan {plan}");
        assert!(!tile.pixels().contains(&0), "plan {plan} tile holds black");
    }
}

#[test]
fn tunnel_masks_are_pixel_doubled() {
    for n in 102..=107 {
        let data = image(n);
        let width = data.size().real_width();
        let rows: Vec<&[u8]> = data.mask().chunks_exact(width).collect();
        assert_eq!(rows.len(), data.size().real_height());
        for (pair, block) in rows.as_chunks::<2>().0.iter().enumerate() {
            let [top, bottom] = block;
            assert_eq!(
                top,
                bottom,
                "mask {n} rows {} and {}",
                2 * pair,
                2 * pair + 1
            );
            for (column, cell) in top.as_chunks::<2>().0.iter().enumerate() {
                assert!(
                    cell[0] == cell[1],
                    "mask {n} row {} columns {} and {}",
                    2 * pair,
                    2 * column,
                    2 * column + 1
                );
            }
        }
    }
}
