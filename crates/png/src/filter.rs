use crate::PngError;

/// The filter types of filter method 0, which PNG applies to each scanline before compression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    None = 0,
    Sub = 1,
    Up = 2,
    Average = 3,
    Paeth = 4,
}

impl Filter {
    /// The filter of a type byte, or `None` for an undefined type.
    pub fn from_byte(byte: u8) -> Option<Filter> {
        match byte {
            0 => Some(Filter::None),
            1 => Some(Filter::Sub),
            2 => Some(Filter::Up),
            3 => Some(Filter::Average),
            4 => Some(Filter::Paeth),
            _ => None,
        }
    }

    /// The Paeth predictor of the specification, with the comparisons in its order.
    pub fn paeth(a: u8, b: u8, c: u8) -> u8 {
        let p = i16::from(a) + i16::from(b) - i16::from(c);
        let pa = (p - i16::from(a)).abs();
        let pb = (p - i16::from(b)).abs();
        let pc = (p - i16::from(c)).abs();
        if pa <= pb && pa <= pc {
            a
        } else if pb <= pc {
            b
        } else {
            c
        }
    }

    /// Reverses the filter on `row` in place. `prior` is the reconstructed previous scanline of the
    /// same pass, or empty for the first one (bytes missing from it count as zero). `bpp` is the
    /// number of bytes per complete pixel, rounded up to 1.
    pub fn unfilter(self, row: &mut [u8], prior: &[u8], bpp: usize) {
        let bpp = bpp.max(1);
        let above = |i: usize| prior.get(i).copied().unwrap_or(0);
        match self {
            Filter::None => {}
            Filter::Sub => {
                for i in bpp..row.len() {
                    row[i] = row[i].wrapping_add(row[i - bpp]);
                }
            }
            Filter::Up => {
                for (i, byte) in row.iter_mut().enumerate() {
                    *byte = byte.wrapping_add(above(i));
                }
            }
            Filter::Average => {
                for i in 0..row.len() {
                    let left = if i >= bpp { row[i - bpp] } else { 0 };
                    let mean = ((u16::from(left) + u16::from(above(i))) / 2) as u8;
                    row[i] = row[i].wrapping_add(mean);
                }
            }
            Filter::Paeth => {
                for i in 0..row.len() {
                    let (left, upper_left) = if i >= bpp { (row[i - bpp], above(i - bpp)) } else { (0, 0) };
                    row[i] = row[i].wrapping_add(Self::paeth(left, above(i), upper_left));
                }
            }
        }
    }

    /// Reverses the filters of consecutive scanlines in place: each is a filter type byte followed
    /// by `row_bytes` bytes. `data` holds whole scanlines only; a partial one at the end is left
    /// alone. The first scanline is reconstructed against a prior scanline of zeros.
    pub fn unfilter_scanlines(data: &mut [u8], row_bytes: usize, bpp: usize) -> Result<(), PngError> {
        let mut prior: &[u8] = &[];
        for line in data.chunks_exact_mut(row_bytes + 1) {
            let Some((kind, row)) = line.split_first_mut() else { break };
            let filter = Filter::from_byte(*kind).ok_or(PngError::InvalidFilter(*kind))?;
            filter.unfilter(row, prior, bpp);
            prior = row;
        }
        Ok(())
    }
}
