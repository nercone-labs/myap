use crate::{Chunk, Filter, Pass, PngError};
use miniz_oxide::inflate::TINFLStatus;

/// The colour types of PNG, with the codes of the `IHDR` chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorType {
    Grey = 0,
    Rgb = 2,
    Indexed = 3,
    GreyAlpha = 4,
    Rgba = 6,
}

impl ColorType {
    /// The colour type of an `IHDR` code, or `None` for an undefined code.
    pub fn from_code(code: u8) -> Option<ColorType> {
        match code {
            0 => Some(ColorType::Grey),
            2 => Some(ColorType::Rgb),
            3 => Some(ColorType::Indexed),
            4 => Some(ColorType::GreyAlpha),
            6 => Some(ColorType::Rgba),
            _ => None,
        }
    }

    /// The code of the colour type in `IHDR`.
    pub fn code(self) -> u8 {
        self as u8
    }

    /// The number of samples per pixel.
    pub fn channels(self) -> u8 {
        match self {
            ColorType::Grey | ColorType::Indexed => 1,
            ColorType::GreyAlpha => 2,
            ColorType::Rgb => 3,
            ColorType::Rgba => 4,
        }
    }

    /// Whether the specification allows `bit_depth` for this colour type (its Table 12).
    pub fn allows(self, bit_depth: u8) -> bool {
        match self {
            ColorType::Grey => matches!(bit_depth, 1 | 2 | 4 | 8 | 16),
            ColorType::Indexed => matches!(bit_depth, 1 | 2 | 4 | 8),
            ColorType::Rgb | ColorType::GreyAlpha | ColorType::Rgba => matches!(bit_depth, 8 | 16),
        }
    }
}

/// The contents of a `tRNS` chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transparency {
    /// Greyscale: pixels of this grey sample are fully transparent.
    Grey(u16),
    /// Truecolour: pixels of these red, green and blue samples are fully transparent.
    Rgb(u16, u16, u16),
    /// Indexed colour: the alpha of the first palette entries; the others are opaque.
    Alpha(Vec<u8>),
}

/// The largest image a decoder accepts. They are checked against the header before any memory
/// is allocated for the image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_width: u32,
    pub max_height: u32,
    /// The largest image data, inflated: the decoded samples plus one filter byte per scanline.
    pub max_bytes: u64,
}

impl Default for Limits {
    /// 16384 by 16384 pixels and 256 MiB of image data.
    fn default() -> Self {
        Limits { max_width: 16384, max_height: 16384, max_bytes: 256 * 1024 * 1024 }
    }
}

/// A decoded PNG image.
///
/// The samples are kept as PNG stores them, without the filter bytes and de-interlaced: rows of
/// [`Image::row_bytes`] bytes from the top, pixels from the left, sub-byte samples packed from
/// the most significant bit, 16-bit samples big-endian.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub color: ColorType,
    /// Bits per sample (per palette index for [`ColorType::Indexed`]).
    pub bit_depth: u8,
    /// Whether the datastream was interlaced with Adam7. The samples are de-interlaced either way.
    pub interlaced: bool,
    /// The `PLTE` chunk: required for indexed colour, a suggestion for truecolour, else empty.
    pub palette: Vec<[u8; 3]>,
    pub transparency: Option<Transparency>,
    samples: Vec<u8>,
}

impl Image {
    /// The eight bytes every PNG datastream starts with.
    pub const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    /// The largest width or height the specification allows.
    pub const MAX_DIMENSION: u32 = 0x7FFF_FFFF;

    /// Decodes a whole PNG datastream with the default [`Limits`].
    pub fn decode(bytes: &[u8]) -> Result<Image, PngError> {
        Self::decode_with(bytes, &Limits::default())
    }

    /// Decodes a whole PNG datastream, refusing images larger than `limits`.
    pub fn decode_with(bytes: &[u8], limits: &Limits) -> Result<Image, PngError> {
        let rest = bytes.strip_prefix(&Self::SIGNATURE).ok_or(PngError::Signature)?;
        if rest.is_empty() {
            return Err(PngError::MissingChunk("IHDR"));
        }
        let (header, mut rest) = Chunk::parse(rest)?;
        if &header.kind != b"IHDR" {
            return Err(PngError::Order("IHDR is not the first chunk"));
        }
        let mut image = Self::from_header(header.data, limits)?;
        let mut data = Vec::new();
        let (mut idat_seen, mut idat_ended, mut palette_seen) = (false, false, false);
        loop {
            if rest.is_empty() {
                return Err(PngError::MissingChunk("IEND"));
            }
            let (chunk, next) = Chunk::parse(rest)?;
            rest = next;
            match &chunk.kind {
                b"IHDR" => return Err(PngError::Order("more than one IHDR")),
                b"PLTE" => {
                    if palette_seen {
                        return Err(PngError::Order("more than one PLTE"));
                    }
                    if idat_seen {
                        return Err(PngError::Order("PLTE after IDAT"));
                    }
                    if image.transparency.is_some() {
                        return Err(PngError::Order("PLTE after tRNS"));
                    }
                    image.read_palette(chunk.data)?;
                    palette_seen = true;
                }
                b"tRNS" => {
                    if idat_seen {
                        return Err(PngError::Order("tRNS after IDAT"));
                    }
                    if image.transparency.is_some() {
                        return Err(PngError::Order("more than one tRNS"));
                    }
                    if image.color == ColorType::Indexed && !palette_seen {
                        return Err(PngError::Order("tRNS before PLTE"));
                    }
                    image.read_transparency(chunk.data)?;
                }
                b"IDAT" => {
                    if idat_ended {
                        return Err(PngError::Order("IDAT chunks are not consecutive"));
                    }
                    idat_seen = true;
                    data.extend_from_slice(chunk.data);
                }
                b"IEND" => break,
                _ if chunk.is_critical() => return Err(PngError::UnknownCriticalChunk(chunk.kind)),
                _ => {}
            }
            if idat_seen && &chunk.kind != b"IDAT" {
                idat_ended = true;
            }
        }
        if !idat_seen {
            return Err(PngError::MissingChunk("IDAT"));
        }
        if image.color == ColorType::Indexed && !palette_seen {
            return Err(PngError::MissingChunk("PLTE"));
        }
        image.samples = image.read_data(&data)?;
        Ok(image)
    }

    /// The image of an `IHDR` chunk, without samples yet.
    fn from_header(data: &[u8], limits: &Limits) -> Result<Image, PngError> {
        let fields: &[u8; 13] = data.try_into().map_err(|_| PngError::InvalidHeader("length is not 13"))?;
        let width = u32::from_be_bytes([fields[0], fields[1], fields[2], fields[3]]);
        let height = u32::from_be_bytes([fields[4], fields[5], fields[6], fields[7]]);
        let [bit_depth, color, compression, filter, interlace] = [fields[8], fields[9], fields[10], fields[11], fields[12]];
        if width == 0 || height == 0 || width > Self::MAX_DIMENSION || height > Self::MAX_DIMENSION {
            return Err(PngError::InvalidHeader("width or height out of range"));
        }
        let color = ColorType::from_code(color).ok_or(PngError::InvalidHeader("unknown colour type"))?;
        if !color.allows(bit_depth) {
            return Err(PngError::InvalidHeader("bit depth not allowed for the colour type"));
        }
        if compression != 0 {
            return Err(PngError::InvalidHeader("unknown compression method"));
        }
        if filter != 0 {
            return Err(PngError::InvalidHeader("unknown filter method"));
        }
        let interlaced = match interlace {
            0 => false,
            1 => true,
            _ => return Err(PngError::InvalidHeader("unknown interlace method")),
        };
        let image = Image { width, height, color, bit_depth, interlaced, palette: Vec::new(), transparency: None, samples: Vec::new() };
        let too_large = PngError::TooLarge { width, height };
        if width > limits.max_width || height > limits.max_height {
            return Err(too_large);
        }
        let within = |length: Option<u64>| length.is_some_and(|n| n <= limits.max_bytes);
        if !within(image.filtered_length()) || !within(image.decoded_length()) {
            return Err(too_large);
        }
        Ok(image)
    }

    fn read_palette(&mut self, data: &[u8]) -> Result<(), PngError> {
        if data.is_empty() || !data.len().is_multiple_of(3) || data.len() > 256 * 3 {
            return Err(PngError::InvalidChunk("PLTE length"));
        }
        match self.color {
            ColorType::Grey | ColorType::GreyAlpha => return Err(PngError::InvalidChunk("PLTE in a greyscale image")),
            ColorType::Indexed if data.len() / 3 > 1 << self.bit_depth => return Err(PngError::InvalidChunk("PLTE larger than the bit depth allows")),
            _ => {}
        }
        self.palette = data.as_chunks::<3>().0.to_vec();
        Ok(())
    }

    fn read_transparency(&mut self, data: &[u8]) -> Result<(), PngError> {
        let transparency = match (self.color, data) {
            (ColorType::Grey, &[g0, g1]) => Transparency::Grey(u16::from_be_bytes([g0, g1])),
            (ColorType::Rgb, &[r0, r1, g0, g1, b0, b1]) => {
                Transparency::Rgb(u16::from_be_bytes([r0, r1]), u16::from_be_bytes([g0, g1]), u16::from_be_bytes([b0, b1]))
            }
            (ColorType::Indexed, alpha) if alpha.len() <= self.palette.len() => Transparency::Alpha(alpha.to_vec()),
            (ColorType::GreyAlpha | ColorType::Rgba, _) => return Err(PngError::InvalidChunk("tRNS in an image with an alpha channel")),
            _ => return Err(PngError::InvalidChunk("tRNS length")),
        };
        self.transparency = Some(transparency);
        Ok(())
    }

    /// Inflates the concatenated `IDAT` data, reverses the filters and de-interlaces.
    fn read_data(&self, compressed: &[u8]) -> Result<Vec<u8>, PngError> {
        let too_large = PngError::TooLarge { width: self.width, height: self.height };
        let expected = self.filtered_length().and_then(|n| usize::try_from(n).ok()).ok_or(too_large)?;
        // One byte more than the image needs, so that longer data is told apart from exact data.
        let limit = expected.checked_add(1).ok_or(too_large)?;
        let mut data = match miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(compressed, limit) {
            Ok(data) => data,
            Err(e) if e.status == TINFLStatus::HasMoreOutput => return Err(PngError::ExtraData),
            Err(_) => return Err(PngError::Inflate),
        };
        if data.len() > expected {
            return Err(PngError::ExtraData);
        }
        if data.len() < expected {
            return Err(PngError::MissingData);
        }
        let bits = self.bits_per_pixel();
        let bpp = (bits as usize).div_ceil(8);
        let row_bytes = self.row_bytes();
        if !self.interlaced {
            Filter::unfilter_scanlines(&mut data, row_bytes, bpp)?;
            // Drop the filter byte in front of every row, in place.
            let mut end = 0;
            for row in 0..self.height as usize {
                let start = row * (row_bytes + 1) + 1;
                data.copy_within(start..start + row_bytes, end);
                end += row_bytes;
            }
            data.truncate(end);
            return Ok(data);
        }
        let decoded = self.decoded_length().and_then(|n| usize::try_from(n).ok()).ok_or(too_large)?;
        let mut samples = vec![0u8; decoded];
        let mut offset = 0;
        for pass in Pass::ADAM7 {
            let (width, height) = pass.size(self.width, self.height);
            if width == 0 || height == 0 {
                continue;
            }
            let pass_row_bytes = Self::row_length(width, bits) as usize;
            let length = (pass_row_bytes + 1) * height as usize;
            let pass_data = data.get_mut(offset..offset + length).ok_or(PngError::MissingData)?;
            Filter::unfilter_scanlines(pass_data, pass_row_bytes, bpp)?;
            for (j, line) in pass_data.chunks_exact(pass_row_bytes + 1).enumerate() {
                let y = (pass.y0 + j as u32 * pass.dy) as usize;
                let target = samples.get_mut(y * row_bytes..(y + 1) * row_bytes).ok_or(PngError::MissingData)?;
                let source = line.get(1..).unwrap_or_default();
                for i in 0..width {
                    let x = pass.x0 + i * pass.dx;
                    Self::copy_pixel(source, i, target, x, bits);
                }
            }
            offset += length;
        }
        Ok(samples)
    }

    /// Copies pixel `from` of the row `source` to pixel `to` of the row `target`, for pixels of
    /// `bits` bits. Pixels outside either row are left alone.
    fn copy_pixel(source: &[u8], from: u32, target: &mut [u8], to: u32, bits: u32) {
        if bits >= 8 {
            let size = (bits / 8) as usize;
            let (from, to) = (from as usize * size, to as usize * size);
            if let (Some(source), Some(target)) = (source.get(from..from + size), target.get_mut(to..to + size)) {
                target.copy_from_slice(source);
            }
            return;
        }
        let mask = ((1u16 << bits) - 1) as u8;
        let (from_bit, to_bit) = (from as usize * bits as usize, to as usize * bits as usize);
        let (Some(&byte), Some(slot)) = (source.get(from_bit / 8), target.get_mut(to_bit / 8)) else { return };
        let value = (byte >> (8 - bits as usize - from_bit % 8)) & mask;
        let shift = 8 - bits as usize - to_bit % 8;
        *slot = (*slot & !(mask << shift)) | (value << shift);
    }

    /// The number of bytes of a row of `width` pixels of `bits_per_pixel` bits, rounded up.
    pub fn row_length(width: u32, bits_per_pixel: u32) -> u64 {
        (u64::from(width) * u64::from(bits_per_pixel)).div_ceil(8)
    }

    /// The number of bits per pixel: samples per pixel times the bit depth.
    pub fn bits_per_pixel(&self) -> u32 {
        u32::from(self.color.channels()) * u32::from(self.bit_depth)
    }

    /// The number of bytes of a row of [`Image::samples`].
    pub fn row_bytes(&self) -> usize {
        Self::row_length(self.width, self.bits_per_pixel()) as usize
    }

    /// The length of the inflated image data: every scanline of every pass with its filter byte,
    /// or `None` if it does not fit in 64 bits.
    pub fn filtered_length(&self) -> Option<u64> {
        let bits = self.bits_per_pixel();
        let length = |(width, height): (u32, u32)| {
            if width == 0 || height == 0 { Some(0) } else { (Self::row_length(width, bits) + 1).checked_mul(u64::from(height)) }
        };
        if self.interlaced {
            Pass::ADAM7.iter().map(|pass| length(pass.size(self.width, self.height))).try_fold(0u64, |sum, n| sum.checked_add(n?))
        } else {
            length((self.width, self.height))
        }
    }

    /// The length of the decoded samples, or `None` if it does not fit in 64 bits.
    pub fn decoded_length(&self) -> Option<u64> {
        Self::row_length(self.width, self.bits_per_pixel()).checked_mul(u64::from(self.height))
    }

    /// The decoded samples, laid out as the documentation of [`Image`] describes.
    pub fn samples(&self) -> &[u8] {
        &self.samples
    }

    /// The raw value of sample `channel` of the pixel at column `x` and row `y` (for indexed
    /// colour, the palette index), or `None` outside the image.
    pub fn sample(&self, x: u32, y: u32, channel: u8) -> Option<u16> {
        let channels = self.color.channels();
        if x >= self.width || y >= self.height || channel >= channels {
            return None;
        }
        let row_bytes = self.row_bytes();
        let start = (y as usize).checked_mul(row_bytes)?;
        let row = self.samples.get(start..start.checked_add(row_bytes)?)?;
        let index = x as usize * usize::from(channels) + usize::from(channel);
        match self.bit_depth {
            16 => Some(u16::from_be_bytes([*row.get(index * 2)?, *row.get(index * 2 + 1)?])),
            8 => row.get(index).map(|&v| u16::from(v)),
            bits @ (1 | 2 | 4) => {
                let bit = index * usize::from(bits);
                let byte = *row.get(bit / 8)?;
                let mask = (1u8 << bits) - 1;
                Some(u16::from((byte >> (8 - usize::from(bits) - bit % 8)) & mask))
            }
            _ => None,
        }
    }

    /// A raw sample scaled to 8 bits: 16-bit samples keep their high byte, samples of 1, 2 and 4
    /// bits are scaled linearly so that their maximum is 255.
    fn to_8_bits(&self, value: u16) -> u8 {
        match self.bit_depth {
            16 => (value >> 8) as u8,
            1 | 2 | 4 => (u32::from(value) * 255 / ((1u32 << self.bit_depth) - 1)) as u8,
            _ => value as u8,
        }
    }

    /// The colour of the pixel at column `x` and row `y` as 8-bit red, green, blue and alpha, or
    /// `None` outside the image. Palettes are looked up, `tRNS` is applied (compared to the raw
    /// samples), grey is copied to the three colours and other bit depths are scaled to 8 bits.
    /// A palette index beyond the palette is opaque black, as the specification requires.
    pub fn rgba8(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        let mask = if self.bit_depth == 16 { 0xFFFF } else { (1u16 << self.bit_depth) - 1 };
        match self.color {
            ColorType::Grey => {
                let grey = self.sample(x, y, 0)?;
                let transparent = matches!(self.transparency, Some(Transparency::Grey(t)) if t & mask == grey);
                let g = self.to_8_bits(grey);
                Some([g, g, g, if transparent { 0 } else { 255 }])
            }
            ColorType::Rgb => {
                let (r, g, b) = (self.sample(x, y, 0)?, self.sample(x, y, 1)?, self.sample(x, y, 2)?);
                let transparent = matches!(self.transparency, Some(Transparency::Rgb(tr, tg, tb)) if (tr & mask, tg & mask, tb & mask) == (r, g, b));
                Some([self.to_8_bits(r), self.to_8_bits(g), self.to_8_bits(b), if transparent { 0 } else { 255 }])
            }
            ColorType::Indexed => {
                let index = usize::from(self.sample(x, y, 0)?);
                let Some(&[r, g, b]) = self.palette.get(index) else { return Some([0, 0, 0, 255]) };
                let alpha = match &self.transparency {
                    Some(Transparency::Alpha(alpha)) => alpha.get(index).copied().unwrap_or(255),
                    _ => 255,
                };
                Some([r, g, b, alpha])
            }
            ColorType::GreyAlpha => {
                let g = self.to_8_bits(self.sample(x, y, 0)?);
                Some([g, g, g, self.to_8_bits(self.sample(x, y, 1)?)])
            }
            ColorType::Rgba => {
                let [r, g, b, a] = [0, 1, 2, 3].map(|c| self.sample(x, y, c).map(|v| self.to_8_bits(v)));
                Some([r?, g?, b?, a?])
            }
        }
    }

    /// The colour of the pixel at column `x` and row `y` as 8-bit red, green and blue, as
    /// [`Image::rgba8`] gives it with the alpha dropped (not composited on any background).
    pub fn rgb8(&self, x: u32, y: u32) -> Option<[u8; 3]> {
        self.rgba8(x, y).map(|[r, g, b, _]| [r, g, b])
    }
}
