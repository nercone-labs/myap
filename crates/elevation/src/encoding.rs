use serde::{Deserialize, Serialize};

/// How the red, green and blue of a pixel hold a height in metres.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Encoding {
    /// Terrarium (Mapzen, AWS Open Data Terrain Tiles): `(R * 256 + G + B / 256) - 32768`.
    /// Every colour is a height; the format has no no-data value.
    Terrarium,
    /// GSI Japan: `x = R * 2^16 + G * 2^8 + B`, a 24-bit two's complement number of
    /// centimetres; `x = 2^23` (`(128, 0, 0)`) is no data.
    Gsi,
}

impl Encoding {
    /// The colour of no data in [`Encoding::Gsi`].
    pub const GSI_NO_DATA: [u8; 3] = [128, 0, 0];
    /// The resolution of [`Encoding::Gsi`] in metres (`u` in the specification).
    pub const GSI_UNIT_M: f64 = 0.01;
    /// The resolution of [`Encoding::Terrarium`] in metres.
    pub const TERRARIUM_UNIT_M: f64 = 1.0 / 256.0;

    /// The height of a colour in metres, or `None` for no data.
    pub fn height(self, rgb: [u8; 3]) -> Option<f64> {
        let [r, g, b] = rgb.map(f64::from);
        match self {
            Encoding::Terrarium => Some(r * 256.0 + g + b / 256.0 - 32768.0),
            Encoding::Gsi => {
                let x = i32::from(rgb[0]) << 16 | i32::from(rgb[1]) << 8 | i32::from(rgb[2]);
                // Dividing by 100 rather than multiplying by 0.01 gives the nearest f64 to the
                // decimal value (0.01 itself is not exact).
                match x.cmp(&(1 << 23)) {
                    std::cmp::Ordering::Less => Some(f64::from(x) / 100.0),
                    std::cmp::Ordering::Equal => None,
                    std::cmp::Ordering::Greater => Some(f64::from(x - (1 << 24)) / 100.0),
                }
            }
        }
    }

    /// The colour of a height in metres, rounded down to the resolution for Terrarium and to the
    /// nearest centimetre for GSI; `None` if the encoding cannot hold it.
    pub fn color(self, metres: f64) -> Option<[u8; 3]> {
        if !metres.is_finite() {
            return None;
        }
        match self {
            Encoding::Terrarium => {
                let v = ((metres + 32768.0) * 256.0).floor();
                if !(0.0..f64::from(1u32 << 24)).contains(&v) {
                    return None;
                }
                let v = v as u32;
                Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
            }
            Encoding::Gsi => {
                let x = (metres * 100.0).round();
                let max = f64::from((1u32 << 23) - 1);
                if !(-max..=max).contains(&x) {
                    return None;
                }
                let x = (x as i32).rem_euclid(1 << 24) as u32;
                Some([(x >> 16) as u8, (x >> 8) as u8, x as u8])
            }
        }
    }
}
