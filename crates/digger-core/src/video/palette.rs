//! The two palettes and two intensities that translate picture indices to
//! colours (research 03 §3).

/// A colour, 0–255 per channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rgb {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
}

impl Rgb {
    /// The colour (`r`, `g`, `b`).
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// One of the two palettes. The game runs in palette 0; palette 1 only
/// appears in the high-score flash (research 03 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Palette {
    /// Palette 0, the game palette.
    #[default]
    Zero,
    /// Palette 1: palette 0 with the index bits rotated.
    One,
}

impl Palette {
    /// The palette numbered `n`, or `None` for a number other than 0 or 1.
    pub const fn from_number(n: u8) -> Option<Self> {
        match n {
            0 => Some(Self::Zero),
            1 => Some(Self::One),
            _ => None,
        }
    }

    /// The palette's number, 0 or 1.
    pub const fn number(self) -> u8 {
        match self {
            Self::Zero => 0,
            Self::One => 1,
        }
    }
}

/// Normal or high intensity. High intensity is the bonus-mode look
/// (research 03 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Intensity {
    /// The normal colours.
    #[default]
    Normal,
    /// The intense colours.
    High,
}

/// The 16 colours of `palette` at `intensity`.
pub const fn colors(palette: Palette, intensity: Intensity) -> &'static [Rgb; 16] {
    match (palette, intensity) {
        (Palette::Zero, Intensity::Normal) => &PALETTE_0_NORMAL,
        (Palette::Zero, Intensity::High) => &PALETTE_0_HIGH,
        (Palette::One, Intensity::Normal) => &PALETTE_1_NORMAL,
        (Palette::One, Intensity::High) => &PALETTE_1_HIGH,
    }
}

const fn c(r: u8, g: u8, b: u8) -> Rgb {
    Rgb::new(r, g, b)
}

/// Palette 0 at normal intensity (research 03 §3, the SDL values).
const PALETTE_0_NORMAL: [Rgb; 16] = [
    c(0, 0, 0),
    c(0, 0, 128),
    c(0, 128, 0),
    c(0, 128, 128),
    c(128, 0, 0),
    c(128, 0, 128),
    c(128, 64, 0),
    c(128, 128, 128),
    c(64, 64, 64),
    c(0, 0, 255),
    c(0, 255, 0),
    c(0, 255, 255),
    c(255, 0, 0),
    c(255, 0, 255),
    c(255, 255, 0),
    c(255, 255, 255),
];

/// Palette 0 at high intensity.
const PALETTE_0_HIGH: [Rgb; 16] = [
    c(0, 0, 0),
    c(0, 0, 255),
    c(0, 255, 0),
    c(0, 255, 255),
    c(255, 0, 0),
    c(255, 0, 255),
    c(255, 128, 0),
    c(196, 196, 196),
    c(128, 128, 128),
    c(128, 128, 255),
    c(128, 255, 128),
    c(128, 255, 255),
    c(255, 128, 128),
    c(255, 128, 255),
    c(255, 255, 128),
    c(255, 255, 255),
];

/// Palette 1 at normal intensity.
const PALETTE_1_NORMAL: [Rgb; 16] = [
    c(0, 0, 0),
    c(0, 128, 0),
    c(128, 0, 0),
    c(128, 64, 0),
    c(0, 0, 128),
    c(0, 128, 128),
    c(128, 0, 128),
    c(128, 128, 128),
    c(64, 64, 64),
    c(0, 255, 0),
    c(255, 0, 0),
    c(255, 255, 0),
    c(0, 0, 255),
    c(0, 255, 255),
    c(255, 0, 255),
    c(255, 255, 255),
];

/// Palette 1 at high intensity.
const PALETTE_1_HIGH: [Rgb; 16] = [
    c(0, 0, 0),
    c(0, 255, 0),
    c(255, 0, 0),
    c(255, 128, 0),
    c(0, 0, 255),
    c(0, 255, 255),
    c(255, 0, 255),
    c(196, 196, 196),
    c(128, 128, 128),
    c(128, 255, 128),
    c(255, 128, 128),
    c(255, 255, 128),
    c(128, 128, 255),
    c(128, 255, 255),
    c(255, 128, 255),
    c(255, 255, 255),
];
