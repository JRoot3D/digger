//! CRC-32 (IEEE 802.3), the frame checksum (research 03 §12.3).

/// The reflected IEEE 802.3 polynomial.
const POLYNOMIAL: u32 = 0xEDB8_8320;

/// The byte-wise lookup table, built at compile time.
static TABLE: [u32; 256] = build_table();

const fn build_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut n: u32 = 0;
    while n < 256 {
        let mut value = n;
        let mut bit = 0;
        while bit < 8 {
            value = if value & 1 == 1 {
                (value >> 1) ^ POLYNOMIAL
            } else {
                value >> 1
            };
            bit += 1;
        }
        #[expect(
            clippy::indexing_slicing,
            reason = "const evaluation: n is below the table length 256"
        )]
        {
            table[n as usize] = value;
        }
        n += 1;
    }
    table
}

/// The CRC-32 of `data`: IEEE 802.3 polynomial, reflected, initial value and
/// final XOR 0xFFFFFFFF (the checksum of zlib, PNG and Ethernet).
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in data {
        let [low, ..] = crc.to_le_bytes();
        let slot = TABLE
            .get(usize::from(low ^ byte))
            .copied()
            .unwrap_or_default();
        crc = (crc >> 8) ^ slot;
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_known_vectors() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b"a"), 0xE8B7_BE43);
        assert_eq!(
            crc32(b"The quick brown fox jumps over the lazy dog"),
            0x414F_A339
        );
    }

    #[test]
    fn crc32_table_endpoints() {
        assert_eq!(TABLE.first().copied(), Some(0));
        assert_eq!(TABLE.get(1).copied(), Some(0x7707_3096));
        assert_eq!(TABLE.last().copied(), Some(0x2D02_EF8D));
    }
}
