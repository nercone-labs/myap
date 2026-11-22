//! Tests against the W3C PNG specification (<https://www.w3.org/TR/png-3/>). The PNG files are
//! built here by a small encoder written from the specification, independent of the decoder.

use png::{ColorType, Crc32, Filter, Image, Limits, Pass, PngError, Transparency};

/// A minimal PNG encoder written from the specification.
struct Encoder;

impl Encoder {
    const SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

    /// The Adam7 pattern of the specification (Figure "Adam7 pass pattern"): the pass of each
    /// pixel of an 8 by 8 block.
    const ADAM7: [[u8; 8]; 8] = [
        [1, 6, 4, 6, 2, 6, 4, 6],
        [7, 7, 7, 7, 7, 7, 7, 7],
        [5, 6, 5, 6, 5, 6, 5, 6],
        [7, 7, 7, 7, 7, 7, 7, 7],
        [3, 6, 4, 6, 3, 6, 4, 6],
        [7, 7, 7, 7, 7, 7, 7, 7],
        [5, 6, 5, 6, 5, 6, 5, 6],
        [7, 7, 7, 7, 7, 7, 7, 7],
    ];

    /// The CRC of the specification (5.5 and Annex D), bit by bit.
    fn crc(bytes: &[u8]) -> u32 {
        let mut c: u32 = 0xFFFF_FFFF;
        for &b in bytes {
            c ^= u32::from(b);
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        c ^ 0xFFFF_FFFF
    }

    fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&Self::crc(&body).to_be_bytes());
        out
    }

    fn ihdr(width: u32, height: u32, bit_depth: u8, color: u8, interlace: u8) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&width.to_be_bytes());
        data.extend_from_slice(&height.to_be_bytes());
        data.extend_from_slice(&[bit_depth, color, 0, 0, interlace]);
        Self::chunk(b"IHDR", &data)
    }

    /// A whole file: signature, IHDR, the chunks of `before` (PLTE, tRNS, ...), the zlib
    /// compression of `filtered` in one IDAT, IEND.
    fn file(ihdr: Vec<u8>, before: &[Vec<u8>], filtered: &[u8]) -> Vec<u8> {
        let mut out = Self::SIGNATURE.to_vec();
        out.extend(ihdr);
        for chunk in before {
            out.extend_from_slice(chunk);
        }
        out.extend(Self::chunk(b"IDAT", &miniz_oxide::deflate::compress_to_vec_zlib(filtered, 6)));
        out.extend(Self::chunk(b"IEND", &[]));
        out
    }

    fn channels(color: u8) -> usize {
        match color {
            0 | 3 => 1,
            4 => 2,
            2 => 3,
            6 => 4,
            _ => unreachable!(),
        }
    }

    /// Packs one row of samples (`channels` per pixel) at `depth` bits each, as 7.2 specifies:
    /// sub-byte samples from the high-order bits, 16-bit samples most significant byte first.
    fn pack(samples: &[u16], depth: u8) -> Vec<u8> {
        match depth {
            16 => samples.iter().flat_map(|s| s.to_be_bytes()).collect(),
            8 => samples.iter().map(|&s| s as u8).collect(),
            d => {
                let per_byte = 8 / d as usize;
                samples
                    .chunks(per_byte)
                    .map(|group| {
                        let mut byte = 0u8;
                        for (i, &s) in group.iter().enumerate() {
                            byte |= (s as u8) << (8 - d as usize * (i + 1));
                        }
                        byte
                    })
                    .collect()
            }
        }
    }

    /// The Paeth predictor of 9.4.
    fn paeth(a: u8, b: u8, c: u8) -> u8 {
        let p = a as i32 + b as i32 - c as i32;
        let (pa, pb, pc) = ((p - a as i32).abs(), (p - b as i32).abs(), (p - c as i32).abs());
        if pa <= pb && pa <= pc {
            a
        } else if pb <= pc {
            b
        } else {
            c
        }
    }

    /// Filters one row (Table 11): `prior` is the previous original row, empty for the first.
    fn filter(kind: u8, row: &[u8], prior: &[u8], bpp: usize) -> Vec<u8> {
        let mut out = vec![kind];
        for i in 0..row.len() {
            let a = if i >= bpp { row[i - bpp] } else { 0 };
            let b = prior.get(i).copied().unwrap_or(0);
            let c = if i >= bpp { prior.get(i - bpp).copied().unwrap_or(0) } else { 0 };
            let predictor = match kind {
                0 => 0,
                1 => a,
                2 => b,
                3 => ((a as u16 + b as u16) / 2) as u8,
                4 => Self::paeth(a, b, c),
                _ => unreachable!(),
            };
            out.push(row[i].wrapping_sub(predictor));
        }
        out
    }

    /// The filtered scanlines of a reduced image given as rows of samples; row `j` uses filter
    /// `filters(j)`.
    fn scanlines(rows: &[Vec<u16>], depth: u8, channels: usize, filters: impl Fn(usize) -> u8) -> Vec<u8> {
        let bpp = (channels * depth as usize).div_ceil(8);
        let mut out = Vec::new();
        let mut prior = Vec::new();
        for (j, row) in rows.iter().enumerate() {
            let packed = Self::pack(row, depth);
            out.extend(Self::filter(filters(j), &packed, &prior, bpp));
            prior = packed;
        }
        out
    }

    /// The image `pixels` (rows of samples) non-interlaced, or interlaced with Adam7 by taking
    /// the pixels of each pass from the pattern of the specification.
    fn image_data(pixels: &[Vec<u16>], width: usize, depth: u8, channels: usize, interlaced: bool, filters: impl Fn(usize) -> u8) -> Vec<u8> {
        if !interlaced {
            return Self::scanlines(pixels, depth, channels, filters);
        }
        let mut out = Vec::new();
        for pass in 1..=7u8 {
            let mut rows = Vec::new();
            for (y, row) in pixels.iter().enumerate() {
                let mut reduced = Vec::new();
                for x in 0..width {
                    if Self::ADAM7[y % 8][x % 8] == pass {
                        reduced.extend_from_slice(&row[x * channels..(x + 1) * channels]);
                    }
                }
                if !reduced.is_empty() {
                    rows.push(reduced);
                }
            }
            out.extend(Self::scanlines(&rows, depth, channels, &filters));
        }
        out
    }
}

/// Deterministic pseudo-random samples below `2^depth`.
fn samples(count: usize, depth: u8, seed: u32) -> Vec<u16> {
    let mut state = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    (0..count)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as u64 % (1u64 << depth)) as u16
        })
        .collect()
}

fn rows_of(width: usize, height: usize, channels: usize, depth: u8, seed: u32) -> Vec<Vec<u16>> {
    (0..height).map(|y| samples(width * channels, depth, seed + y as u32)).collect()
}

/// Every colour type and bit depth the specification allows (Table 12).
const COMBINATIONS: [(u8, u8); 15] =
    [(0, 1), (0, 2), (0, 4), (0, 8), (0, 16), (2, 8), (2, 16), (3, 1), (3, 2), (3, 4), (3, 8), (4, 8), (4, 16), (6, 8), (6, 16)];

fn palette_for(depth: u8) -> Vec<u8> {
    (0..(1u32 << depth)).flat_map(|i| [i as u8, (i * 7) as u8, 255 - i as u8]).collect()
}

fn assert_samples(image: &Image, rows: &[Vec<u16>], channels: usize) {
    for (y, row) in rows.iter().enumerate() {
        for x in 0..image.width as usize {
            for c in 0..channels {
                assert_eq!(image.sample(x as u32, y as u32, c as u8), Some(row[x * channels + c]), "pixel ({x}, {y}) channel {c}");
            }
        }
    }
}

fn build(color: u8, depth: u8, width: usize, height: usize, interlaced: bool, filters: impl Fn(usize) -> u8, seed: u32) -> (Vec<u8>, Vec<Vec<u16>>) {
    let channels = Encoder::channels(color);
    let rows = rows_of(width, height, channels, depth, seed);
    let data = Encoder::image_data(&rows, width, depth, channels, interlaced, filters);
    let before = if color == 3 { vec![Encoder::chunk(b"PLTE", &palette_for(depth))] } else { vec![] };
    (Encoder::file(Encoder::ihdr(width as u32, height as u32, depth, color, u8::from(interlaced)), &before, &data), rows)
}

#[test]
fn crc32_check_value() {
    // The standard check value of CRC-32 (ISO 3309 / ITU-T V.42), and the CRC of an IEND chunk,
    // which every PNG file ends with: AE 42 60 82.
    assert_eq!(Crc32::of(b"123456789"), 0xCBF4_3926);
    assert_eq!(Crc32::of(b"IEND"), 0xAE42_6082);
    let mut crc = Crc32::new();
    crc.update(b"1234");
    crc.update(b"56789");
    assert_eq!(crc.value(), 0xCBF4_3926);
}

#[test]
fn every_color_type_and_bit_depth() {
    for (color, depth) in COMBINATIONS {
        // A width that is not a multiple of 8 leaves unused bits at the end of sub-byte rows.
        let (file, rows) = build(color, depth, 13, 5, false, |_| 0, u32::from(color) * 100 + u32::from(depth));
        let image = Image::decode(&file).unwrap_or_else(|e| panic!("colour {color} depth {depth}: {e}"));
        assert_eq!((image.width, image.height, image.color.code(), image.bit_depth), (13, 5, color, depth));
        assert!(!image.interlaced);
        assert_samples(&image, &rows, Encoder::channels(color));
        assert_eq!(image.sample(13, 0, 0), None);
        assert_eq!(image.sample(0, 5, 0), None);
        assert_eq!(image.sample(0, 0, Encoder::channels(color) as u8), None);
        assert_eq!(image.rgba8(13, 0), None);
    }
}

#[test]
fn every_filter_type_on_every_format() {
    for (color, depth) in COMBINATIONS {
        for filter in 0..=4u8 {
            let (file, rows) = build(color, depth, 11, 6, false, |_| filter, 7 + u32::from(filter));
            let image = Image::decode(&file).unwrap_or_else(|e| panic!("colour {color} depth {depth} filter {filter}: {e}"));
            assert_samples(&image, &rows, Encoder::channels(color));
        }
        // Filters changing from row to row.
        let (file, rows) = build(color, depth, 9, 10, false, |j| (j % 5) as u8, 3);
        assert_samples(&Image::decode(&file).unwrap(), &rows, Encoder::channels(color));
    }
}

#[test]
fn filters_by_hand() {
    // Greyscale 8-bit, 3 by 2; the first row unfiltered: [10, 20, 30].
    // Second row [15, 25, 5] with Paeth: the predictors are b = 10, 20, 30 (pb is the smallest
    // every time), so the filtered bytes are 5, 5 and 5 - 30 = 231 (mod 256).
    let file = Encoder::file(Encoder::ihdr(3, 2, 8, 0, 0), &[], &[0, 10, 20, 30, 4, 5, 5, 231]);
    let image = Image::decode(&file).unwrap();
    assert_eq!(image.samples(), &[10, 20, 30, 15, 25, 5]);

    // Average with a sum above 255 (nine-bit arithmetic): first row [200, 100]; second row
    // [250, 50]: predictors floor((0 + 200) / 2) = 100 and floor((250 + 100) / 2) = 175, so the
    // filtered bytes are 150 and 50 - 175 = 131 (mod 256).
    let file = Encoder::file(Encoder::ihdr(2, 2, 8, 0, 0), &[], &[0, 200, 100, 3, 150, 131]);
    assert_eq!(Image::decode(&file).unwrap().samples(), &[200, 100, 250, 50]);

    // Sub on RGB 8-bit: the byte to the left is that of the previous pixel (3 bytes back).
    // Up on the second row. First row [1, 2, 3, 11, 12, 13]; second [2, 4, 6, 12, 14, 16].
    let file = Encoder::file(Encoder::ihdr(2, 2, 8, 2, 0), &[], &[1, 1, 2, 3, 10, 10, 10, 2, 1, 2, 3, 1, 2, 3]);
    assert_eq!(Image::decode(&file).unwrap().samples(), &[1, 2, 3, 11, 12, 13, 2, 4, 6, 12, 14, 16]);

    // Sub on a 1-bit image: the "pixel to the left" is the previous byte (bpp rounds up to 1).
    // Width 16: two bytes per row, [0x0F, 0xF0] filtered as [0x0F, 0xF0 - 0x0F = 0xE1].
    let file = Encoder::file(Encoder::ihdr(16, 1, 1, 0, 0), &[], &[1, 0x0F, 0xE1]);
    assert_eq!(Image::decode(&file).unwrap().samples(), &[0x0F, 0xF0]);

    // Paeth on the first row, where the prior row counts as zeros: it is the same as Sub.
    let file = Encoder::file(Encoder::ihdr(3, 1, 8, 0, 0), &[], &[4, 10, 10, 10]);
    assert_eq!(Image::decode(&file).unwrap().samples(), &[10, 20, 30]);
}

#[test]
fn paeth_predictor() {
    // p = a + b - c; pa = |b - c|, pb = |a - c|, pc = |a + b - 2c|; ties go to a, then b.
    assert_eq!(Filter::paeth(7, 7, 7), 7); // all 0: a
    assert_eq!(Filter::paeth(4, 13, 10), 4); // pa = pc = 3 < pb = 6: a
    assert_eq!(Filter::paeth(13, 4, 10), 4); // pb = pc = 3 < pa = 6: b
    assert_eq!(Filter::paeth(0, 4, 2), 2); // pa = pb = 2, pc = 0: c
    assert_eq!(Filter::paeth(1, 3, 0), 3); // pa = 3, pb = 1, pc = 4: b
    assert_eq!(Filter::paeth(10, 20, 10), 20); // pa = 10, pb = 0: b
    assert_eq!(Filter::paeth(255, 255, 0), 255); // p = 510 without overflow: pa = pb = 255 < pc
    assert_eq!(Filter::paeth(0, 0, 255), 0); // p = -255 without overflow: pa = pb = 255 < pc
}

#[test]
fn adam7_matches_non_interlaced() {
    for (color, depth) in COMBINATIONS {
        // Sizes with empty passes (1 by 1, 2 by 1, ...), partial blocks and several blocks.
        for (width, height) in [(1, 1), (2, 1), (1, 3), (3, 3), (5, 2), (8, 8), (9, 7), (17, 13)] {
            let seed = u32::from(color) * 31 + u32::from(depth) + width as u32 * 7 + height as u32;
            let (interlaced, rows) = build(color, depth, width, height, true, |j| (j % 5) as u8, seed);
            let (plain, _) = build(color, depth, width, height, false, |_| 0, seed);
            let a = Image::decode(&interlaced).unwrap_or_else(|e| panic!("colour {color} depth {depth} {width}x{height}: {e}"));
            let b = Image::decode(&plain).unwrap();
            assert!(a.interlaced && !b.interlaced);
            assert_eq!(a.samples(), b.samples(), "colour {color} depth {depth} {width}x{height}");
            assert_samples(&a, &rows, Encoder::channels(color));
        }
    }
}

#[test]
fn adam7_pass_sizes() {
    // From the pattern of the specification: an 8 by 8 image has 1, 1, 2, 4, 8, 16 and 32
    // pixels in the passes.
    let counts: Vec<u32> = Pass::ADAM7.iter().map(|p| p.size(8, 8)).map(|(w, h)| w * h).collect();
    assert_eq!(counts, [1, 1, 2, 4, 8, 16, 32]);
    // A 1 by 1 image has only the first pass.
    let sizes: Vec<(u32, u32)> = Pass::ADAM7.iter().map(|p| p.size(1, 1)).collect();
    assert_eq!(sizes, [(1, 1), (0, 1), (1, 0), (0, 1), (1, 0), (0, 1), (1, 0)]);
}

#[test]
fn rgba8_scaling() {
    // Greyscale of 1, 2 and 4 bits scaled linearly to 0..255 (MAXINSAMPLE = 2^depth - 1).
    for (depth, value, expected) in [(1u8, 1u16, 255u8), (1, 0, 0), (2, 1, 85), (2, 2, 170), (2, 3, 255), (4, 7, 119), (4, 15, 255), (4, 1, 17)] {
        let file = Encoder::file(Encoder::ihdr(1, 1, depth, 0, 0), &[], &[&[0u8][..], &Encoder::pack(&[value], depth)].concat());
        assert_eq!(Image::decode(&file).unwrap().rgba8(0, 0), Some([expected, expected, expected, 255]), "depth {depth} value {value}");
    }
    // 16-bit samples keep the high byte (13.12: the low-order byte can be discarded).
    let file = Encoder::file(Encoder::ihdr(1, 1, 16, 6, 0), &[], &[0, 0x12, 0x34, 0xAB, 0xCD, 0x00, 0xFF, 0x80, 0x01]);
    let image = Image::decode(&file).unwrap();
    assert_eq!(image.rgba8(0, 0), Some([0x12, 0xAB, 0x00, 0x80]));
    assert_eq!(image.rgb8(0, 0), Some([0x12, 0xAB, 0x00]));
    // Grey with alpha: the grey copied to the three colours.
    let file = Encoder::file(Encoder::ihdr(1, 1, 8, 4, 0), &[], &[0, 77, 66]);
    assert_eq!(Image::decode(&file).unwrap().rgba8(0, 0), Some([77, 77, 77, 66]));
    // RGB without tRNS is opaque.
    let file = Encoder::file(Encoder::ihdr(1, 1, 8, 2, 0), &[], &[0, 1, 2, 3]);
    assert_eq!(Image::decode(&file).unwrap().rgba8(0, 0), Some([1, 2, 3, 255]));
}

#[test]
fn palette_and_transparency() {
    // Four entries, tRNS for the first two only: the others are opaque (11.3.1.1).
    let palette = Encoder::chunk(b"PLTE", &[255, 0, 0, 0, 255, 0, 0, 0, 255, 9, 9, 9]);
    let trns = Encoder::chunk(b"tRNS", &[0, 128]);
    // 2-bit indices 0, 1, 2 and 3, packed in one byte as 0b00_01_10_11.
    let file = Encoder::file(Encoder::ihdr(4, 1, 2, 3, 0), &[palette.clone(), trns], &[0, 0b0001_1011]);
    let image = Image::decode(&file).unwrap();
    assert_eq!(image.palette.len(), 4);
    assert_eq!(image.transparency, Some(Transparency::Alpha(vec![0, 128])));
    assert_eq!(image.rgba8(0, 0), Some([255, 0, 0, 0]));
    assert_eq!(image.rgba8(1, 0), Some([0, 255, 0, 128]));
    assert_eq!(image.rgba8(2, 0), Some([0, 0, 255, 255]));
    assert_eq!(image.rgba8(3, 0), Some([9, 9, 9, 255]));

    // An index beyond the palette is displayed as opaque black (13.1).
    let short = Encoder::chunk(b"PLTE", &[1, 2, 3]);
    let file = Encoder::file(Encoder::ihdr(2, 1, 8, 3, 0), &[short], &[0, 0, 200]);
    let image = Image::decode(&file).unwrap();
    assert_eq!(image.rgba8(0, 0), Some([1, 2, 3, 255]));
    assert_eq!(image.rgba8(1, 0), Some([0, 0, 0, 255]));

    // Greyscale tRNS: two bytes whatever the depth, the bits above the depth masked off.
    let trns = Encoder::chunk(b"tRNS", &[0x01, 0x07]);
    let file = Encoder::file(Encoder::ihdr(2, 1, 4, 0, 0), &[trns], &[0, 0x75]);
    let image = Image::decode(&file).unwrap();
    assert_eq!(image.rgba8(0, 0), Some([119, 119, 119, 0]));
    assert_eq!(image.rgba8(1, 0), Some([85, 85, 85, 255]));

    // 16-bit greyscale: only the whole 16-bit value is transparent, not the same high byte.
    let trns = Encoder::chunk(b"tRNS", &[0x12, 0x34]);
    let file = Encoder::file(Encoder::ihdr(2, 1, 16, 0, 0), &[trns], &[0, 0x12, 0x34, 0x12, 0x35]);
    let image = Image::decode(&file).unwrap();
    assert_eq!(image.rgba8(0, 0), Some([0x12, 0x12, 0x12, 0]));
    assert_eq!(image.rgba8(1, 0), Some([0x12, 0x12, 0x12, 255]));

    // Truecolour tRNS: all three samples must match.
    let trns = Encoder::chunk(b"tRNS", &[0, 10, 0, 20, 0, 30]);
    let file = Encoder::file(Encoder::ihdr(2, 1, 8, 2, 0), &[trns], &[0, 10, 20, 30, 10, 20, 31]);
    let image = Image::decode(&file).unwrap();
    assert_eq!(image.transparency, Some(Transparency::Rgb(10, 20, 30)));
    assert_eq!(image.rgba8(0, 0), Some([10, 20, 30, 0]));
    assert_eq!(image.rgba8(1, 0), Some([10, 20, 31, 255]));

    // A suggested palette in a truecolour image changes nothing.
    let file = Encoder::file(Encoder::ihdr(1, 1, 8, 2, 0), &[palette], &[0, 4, 5, 6]);
    assert_eq!(Image::decode(&file).unwrap().rgba8(0, 0), Some([4, 5, 6, 255]));
}

#[test]
fn ancillary_chunks_are_ignored_and_idat_may_be_split() {
    let filtered = [0u8, 1, 2, 3, 0, 4, 5, 6];
    let zlib = miniz_oxide::deflate::compress_to_vec_zlib(&filtered, 6);
    let mut file = Encoder::SIGNATURE.to_vec();
    file.extend(Encoder::ihdr(3, 2, 8, 0, 0));
    file.extend(Encoder::chunk(b"tEXt", b"Comment\0hello"));
    file.extend(Encoder::chunk(b"prVt", b"private ancillary"));
    // The zlib stream split at every byte, plus an empty IDAT.
    for byte in &zlib {
        file.extend(Encoder::chunk(b"IDAT", std::slice::from_ref(byte)));
    }
    file.extend(Encoder::chunk(b"IDAT", &[]));
    file.extend(Encoder::chunk(b"tIME", &[0x07, 0xE9, 1, 1, 0, 0, 0]));
    file.extend(Encoder::chunk(b"IEND", &[]));
    assert_eq!(Image::decode(&file).unwrap().samples(), &[1, 2, 3, 4, 5, 6]);
}

fn valid() -> Vec<u8> {
    Encoder::file(Encoder::ihdr(2, 2, 8, 0, 0), &[], &[0, 1, 2, 0, 3, 4])
}

#[test]
fn bad_signature() {
    let mut file = valid();
    file[1] = b'p';
    assert_eq!(Image::decode(&file), Err(PngError::Signature));
    assert_eq!(Image::decode(&[]), Err(PngError::Signature));
    assert_eq!(Image::decode(b"GIF89a"), Err(PngError::Signature));
}

#[test]
fn bad_crc() {
    // Flip a bit of the width in IHDR.
    let mut file = valid();
    file[8 + 8 + 3] ^= 1;
    assert_eq!(Image::decode(&file), Err(PngError::Crc(*b"IHDR")));
    // Flip a bit of the stored CRC of IEND.
    let mut file = valid();
    let last = file.len() - 1;
    file[last] ^= 0x80;
    assert_eq!(Image::decode(&file), Err(PngError::Crc(*b"IEND")));
    // A bad CRC in an ancillary chunk is still a corrupted datastream.
    let mut text = Encoder::chunk(b"tEXt", b"a\0b");
    let end = text.len() - 1;
    text[end] ^= 1;
    let file = Encoder::file(Encoder::ihdr(1, 1, 8, 0, 0), &[text], &[0, 0]);
    assert_eq!(Image::decode(&file), Err(PngError::Crc(*b"tEXt")));
}

#[test]
fn truncated_data() {
    let file = valid();
    for length in [9, 20, 34, file.len() - 13, file.len() - 1] {
        assert_eq!(Image::decode(&file[..length]), Err(PngError::Truncated), "length {length}");
    }
    assert_eq!(Image::decode(&file[..8]), Err(PngError::MissingChunk("IHDR")));
    // Ending right after a whole chunk is a missing IEND rather than a cut chunk.
    assert_eq!(Image::decode(&file[..33]), Err(PngError::MissingChunk("IEND")));
}

#[test]
fn missing_chunks() {
    let file = valid();
    // Without IEND (12 bytes).
    assert_eq!(Image::decode(&file[..file.len() - 12]), Err(PngError::MissingChunk("IEND")));
    // Without IDAT.
    let mut file = Encoder::SIGNATURE.to_vec();
    file.extend(Encoder::ihdr(1, 1, 8, 0, 0));
    file.extend(Encoder::chunk(b"IEND", &[]));
    assert_eq!(Image::decode(&file), Err(PngError::MissingChunk("IDAT")));
    // Indexed colour without PLTE.
    let file = Encoder::file(Encoder::ihdr(1, 1, 8, 3, 0), &[], &[0, 0]);
    assert_eq!(Image::decode(&file), Err(PngError::MissingChunk("PLTE")));
    // IHDR not first.
    let mut file = Encoder::SIGNATURE.to_vec();
    file.extend(Encoder::chunk(b"tEXt", b"a\0b"));
    file.extend(Encoder::ihdr(1, 1, 8, 0, 0));
    assert!(matches!(Image::decode(&file), Err(PngError::Order(_))));
}

#[test]
fn invalid_header() {
    // Every combination outside Table 12, and undefined colour types.
    for color in 0..=7u8 {
        for depth in [0u8, 1, 2, 3, 4, 5, 8, 12, 16, 32] {
            let allowed = COMBINATIONS.contains(&(color, depth));
            let channels = match color {
                0 | 3 => 1,
                4 => 2,
                2 => 3,
                6 => 4,
                _ => 1,
            };
            let row = vec![0u8; 1 + (channels * depth as usize).div_ceil(8).max(1)];
            let before = if color == 3 { vec![Encoder::chunk(b"PLTE", &[0, 0, 0])] } else { vec![] };
            let file = Encoder::file(Encoder::ihdr(1, 1, depth, color, 0), &before, &row);
            let result = Image::decode(&file);
            if allowed {
                assert!(result.is_ok(), "colour {color} depth {depth}: {result:?}");
            } else {
                assert!(matches!(result, Err(PngError::InvalidHeader(_))), "colour {color} depth {depth}: {result:?}");
            }
        }
    }
    let header = |width: u32, height: u32, compression: u8, filter: u8, interlace: u8| {
        let mut data = Vec::new();
        data.extend_from_slice(&width.to_be_bytes());
        data.extend_from_slice(&height.to_be_bytes());
        data.extend_from_slice(&[8, 0, compression, filter, interlace]);
        let mut file = Encoder::SIGNATURE.to_vec();
        file.extend(Encoder::chunk(b"IHDR", &data));
        file.extend(Encoder::chunk(b"IDAT", &miniz_oxide::deflate::compress_to_vec_zlib(&[0, 0], 6)));
        file.extend(Encoder::chunk(b"IEND", &[]));
        Image::decode(&file)
    };
    assert!(header(1, 1, 0, 0, 0).is_ok());
    assert!(matches!(header(0, 1, 0, 0, 0), Err(PngError::InvalidHeader(_))));
    assert!(matches!(header(1, 0, 0, 0, 0), Err(PngError::InvalidHeader(_))));
    assert!(matches!(header(1 << 31, 1, 0, 0, 0), Err(PngError::InvalidHeader(_))));
    assert!(matches!(header(1, 1, 1, 0, 0), Err(PngError::InvalidHeader(_))));
    assert!(matches!(header(1, 1, 0, 1, 0), Err(PngError::InvalidHeader(_))));
    assert!(matches!(header(1, 1, 0, 0, 2), Err(PngError::InvalidHeader(_))));
    // An IHDR of the wrong length.
    let mut file = Encoder::SIGNATURE.to_vec();
    file.extend(Encoder::chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 0, 0, 0]));
    assert!(matches!(Image::decode(&file), Err(PngError::InvalidHeader(_))));
}

#[test]
fn chunk_rules() {
    let idat = |data: &[u8]| Encoder::chunk(b"IDAT", &miniz_oxide::deflate::compress_to_vec_zlib(data, 6));
    let assemble = |chunks: Vec<Vec<u8>>| {
        let mut file = Encoder::SIGNATURE.to_vec();
        for chunk in chunks {
            file.extend(chunk);
        }
        Image::decode(&file)
    };
    let iend = Encoder::chunk(b"IEND", &[]);
    let rgb = || Encoder::ihdr(1, 1, 8, 2, 0);
    let plte = Encoder::chunk(b"PLTE", &[1, 2, 3]);

    // An unknown critical chunk is an error; an unknown ancillary one is not.
    let result = assemble(vec![rgb(), Encoder::chunk(b"CRIT", &[]), idat(&[0, 0, 0, 0]), iend.clone()]);
    assert_eq!(result, Err(PngError::UnknownCriticalChunk(*b"CRIT")));
    assert!(assemble(vec![rgb(), Encoder::chunk(b"cRIT", &[]), idat(&[0, 0, 0, 0]), iend.clone()]).is_ok());
    // IDAT chunks shall be consecutive.
    let zlib = miniz_oxide::deflate::compress_to_vec_zlib(&[0, 0, 0, 0], 6);
    let (first, second) = zlib.split_at(2);
    let result = assemble(vec![rgb(), Encoder::chunk(b"IDAT", first), Encoder::chunk(b"tEXt", b"a\0b"), Encoder::chunk(b"IDAT", second), iend.clone()]);
    assert!(matches!(result, Err(PngError::Order(_))));
    // PLTE before IDAT, at most once, never in greyscale.
    assert!(matches!(assemble(vec![rgb(), idat(&[0, 0, 0, 0]), plte.clone(), iend.clone()]), Err(PngError::Order(_))));
    assert!(matches!(assemble(vec![rgb(), plte.clone(), plte.clone(), idat(&[0, 0, 0, 0]), iend.clone()]), Err(PngError::Order(_))));
    let grey = || Encoder::ihdr(1, 1, 8, 0, 0);
    assert!(matches!(assemble(vec![grey(), plte.clone(), idat(&[0, 0]), iend.clone()]), Err(PngError::InvalidChunk(_))));
    // PLTE length: a multiple of 3, 1 to 256 entries, at most 2^depth entries for indexed colour.
    let indexed = || Encoder::ihdr(1, 1, 1, 3, 0);
    assert!(matches!(assemble(vec![rgb(), Encoder::chunk(b"PLTE", &[1, 2]), idat(&[0, 0, 0, 0]), iend.clone()]), Err(PngError::InvalidChunk(_))));
    assert!(matches!(assemble(vec![rgb(), Encoder::chunk(b"PLTE", &[]), idat(&[0, 0, 0, 0]), iend.clone()]), Err(PngError::InvalidChunk(_))));
    assert!(matches!(assemble(vec![rgb(), Encoder::chunk(b"PLTE", &[0; 257 * 3]), idat(&[0, 0, 0, 0]), iend.clone()]), Err(PngError::InvalidChunk(_))));
    assert!(matches!(assemble(vec![indexed(), Encoder::chunk(b"PLTE", &[0; 9]), idat(&[0, 0]), iend.clone()]), Err(PngError::InvalidChunk(_))));
    assert!(assemble(vec![indexed(), Encoder::chunk(b"PLTE", &[0; 6]), idat(&[0, 0]), iend.clone()]).is_ok());
    // tRNS: after PLTE, before IDAT, no more entries than PLTE, never with an alpha channel.
    let trns = |data: &[u8]| Encoder::chunk(b"tRNS", data);
    let indexed8 = || Encoder::ihdr(1, 1, 8, 3, 0);
    assert!(matches!(assemble(vec![indexed8(), trns(&[0]), plte.clone(), idat(&[0, 0]), iend.clone()]), Err(PngError::Order(_))));
    assert!(matches!(assemble(vec![indexed8(), plte.clone(), idat(&[0, 0]), trns(&[0]), iend.clone()]), Err(PngError::Order(_))));
    assert!(matches!(assemble(vec![indexed8(), plte.clone(), trns(&[0, 0]), idat(&[0, 0]), iend.clone()]), Err(PngError::InvalidChunk(_))));
    assert!(assemble(vec![indexed8(), plte.clone(), trns(&[]), idat(&[0, 0]), iend.clone()]).is_ok());
    assert!(matches!(assemble(vec![Encoder::ihdr(1, 1, 8, 6, 0), trns(&[0, 0]), idat(&[0, 0, 0, 0, 0]), iend.clone()]), Err(PngError::InvalidChunk(_))));
    assert!(matches!(assemble(vec![Encoder::ihdr(1, 1, 8, 4, 0), trns(&[0, 0]), idat(&[0, 0, 0]), iend.clone()]), Err(PngError::InvalidChunk(_))));
    assert!(matches!(assemble(vec![grey(), trns(&[0, 0, 0]), idat(&[0, 0]), iend.clone()]), Err(PngError::InvalidChunk(_))));
    assert!(matches!(assemble(vec![rgb(), trns(&[0, 0]), idat(&[0, 0, 0, 0]), iend.clone()]), Err(PngError::InvalidChunk(_))));
    // A second IHDR.
    assert!(matches!(assemble(vec![rgb(), rgb(), idat(&[0, 0, 0, 0]), iend.clone()]), Err(PngError::Order(_))));
    // A chunk length above 2^31 - 1.
    let mut long = vec![0x80, 0, 0, 0];
    long.extend_from_slice(b"IDAT");
    assert_eq!(assemble(vec![rgb(), long]), Err(PngError::ChunkTooLong(0x8000_0000)));
}

#[test]
fn invalid_image_data() {
    // An undefined filter type.
    let file = Encoder::file(Encoder::ihdr(1, 1, 8, 0, 0), &[], &[5, 0]);
    assert_eq!(Image::decode(&file), Err(PngError::InvalidFilter(5)));
    // Too little data, and more than the image holds.
    let file = Encoder::file(Encoder::ihdr(2, 2, 8, 0, 0), &[], &[0, 1, 2, 0, 3]);
    assert_eq!(Image::decode(&file), Err(PngError::MissingData));
    let file = Encoder::file(Encoder::ihdr(2, 2, 8, 0, 0), &[], &[0, 1, 2, 0, 3, 4, 0]);
    assert_eq!(Image::decode(&file), Err(PngError::ExtraData));
    // Not zlib (raw deflate without the zlib header), and a stream cut short.
    let mut file = Encoder::SIGNATURE.to_vec();
    file.extend(Encoder::ihdr(1, 1, 8, 0, 0));
    file.extend(Encoder::chunk(b"IDAT", &miniz_oxide::deflate::compress_to_vec(&[0, 0], 6)));
    file.extend(Encoder::chunk(b"IEND", &[]));
    assert_eq!(Image::decode(&file), Err(PngError::Inflate));
    let zlib = miniz_oxide::deflate::compress_to_vec_zlib(&[0u8, 1, 2, 0, 3, 4], 6);
    let mut file = Encoder::SIGNATURE.to_vec();
    file.extend(Encoder::ihdr(2, 2, 8, 0, 0));
    file.extend(Encoder::chunk(b"IDAT", &zlib[..zlib.len() - 6]));
    file.extend(Encoder::chunk(b"IEND", &[]));
    assert!(matches!(Image::decode(&file), Err(PngError::Inflate | PngError::MissingData)));
    // A wrong Adler-32 at the end of the zlib stream.
    let mut zlib = zlib.clone();
    let end = zlib.len() - 1;
    zlib[end] ^= 1;
    let mut file = Encoder::SIGNATURE.to_vec();
    file.extend(Encoder::ihdr(2, 2, 8, 0, 0));
    file.extend(Encoder::chunk(b"IDAT", &zlib));
    file.extend(Encoder::chunk(b"IEND", &[]));
    assert_eq!(Image::decode(&file), Err(PngError::Inflate));
}

#[test]
fn decompression_bomb() {
    // A 1 by 1 image needs 2 bytes; this stream of 64 MiB of zeros compresses to about 64 KiB.
    // The decoder must stop at the size of the image instead of inflating it all.
    let zeros = vec![0u8; 64 * 1024 * 1024];
    let file = Encoder::file(Encoder::ihdr(1, 1, 8, 0, 0), &[], &zeros);
    assert!(file.len() < 1024 * 1024);
    assert_eq!(Image::decode(&file), Err(PngError::ExtraData));
}

#[test]
fn limits() {
    // The header alone decides: the IDAT here is tiny and never inflated.
    let file = Encoder::file(Encoder::ihdr(100_000, 1, 8, 0, 0), &[], &[0]);
    assert_eq!(Image::decode(&file), Err(PngError::TooLarge { width: 100_000, height: 1 }));
    let file = Encoder::file(Encoder::ihdr(1, 100_000, 8, 0, 0), &[], &[0]);
    assert_eq!(Image::decode(&file), Err(PngError::TooLarge { width: 1, height: 100_000 }));
    // 16384 by 16384 RGBA of 16 bits is 2 GiB, over 256 MiB.
    let file = Encoder::file(Encoder::ihdr(16384, 16384, 16, 6, 0), &[], &[0]);
    assert_eq!(Image::decode(&file), Err(PngError::TooLarge { width: 16384, height: 16384 }));
    // The largest the specification allows.
    let file = Encoder::file(Encoder::ihdr(0x7FFF_FFFF, 0x7FFF_FFFF, 16, 6, 1), &[], &[0]);
    let limits = Limits { max_width: u32::MAX, max_height: u32::MAX, max_bytes: u64::MAX };
    assert!(matches!(Image::decode_with(&file, &limits), Err(PngError::TooLarge { .. } | PngError::MissingData)));

    let limits = Limits { max_width: 4, max_height: 4, max_bytes: 1024 };
    let file = Encoder::file(Encoder::ihdr(4, 4, 8, 0, 0), &[], &[0; 20]);
    assert!(Image::decode_with(&file, &limits).is_ok());
    let file = Encoder::file(Encoder::ihdr(5, 4, 8, 0, 0), &[], &[0; 24]);
    assert_eq!(Image::decode_with(&file, &limits), Err(PngError::TooLarge { width: 5, height: 4 }));
    // 4 rows of 4 bytes and their filter bytes: 20 bytes.
    let limits = Limits { max_width: 4, max_height: 4, max_bytes: 19 };
    let file = Encoder::file(Encoder::ihdr(4, 4, 8, 0, 0), &[], &[0; 20]);
    assert_eq!(Image::decode_with(&file, &limits), Err(PngError::TooLarge { width: 4, height: 4 }));
    assert_eq!(Limits::default(), Limits { max_width: 16384, max_height: 16384, max_bytes: 256 * 1024 * 1024 });
}

#[test]
fn color_type_rules() {
    assert_eq!(ColorType::from_code(1), None);
    assert_eq!(ColorType::from_code(6), Some(ColorType::Rgba));
    assert_eq!(ColorType::Rgba.channels(), 4);
    assert!(ColorType::Grey.allows(2));
    assert!(!ColorType::Rgb.allows(4));
    assert!(!ColorType::Indexed.allows(16));
}

#[test]
fn never_panics_on_corrupted_files() {
    // Every single-byte corruption and every truncation of a small file: an error or an image,
    // never a panic. CRCs are recomputed so that the corruption reaches the chunk contents.
    let (file, _) = build(3, 2, 9, 7, true, |j| (j % 5) as u8, 5);
    for length in 0..file.len() {
        let _ = Image::decode(&file[..length]);
    }
    let mut chunks = Vec::new();
    let mut rest = &file[8..];
    while rest.len() >= 12 {
        let length = u32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]) as usize;
        chunks.push((rest[4..8].to_vec(), rest[8..8 + length].to_vec()));
        rest = &rest[12 + length..];
    }
    for (c, (_, data)) in chunks.iter().enumerate() {
        for i in 0..data.len() {
            for value in [0u8, 0xFF, data[i] ^ 0x55] {
                let mut out = Encoder::SIGNATURE.to_vec();
                for (k, (kind, d)) in chunks.iter().enumerate() {
                    let mut d = d.clone();
                    if k == c {
                        d[i] = value;
                    }
                    out.extend(Encoder::chunk(&[kind[0], kind[1], kind[2], kind[3]], &d));
                }
                let _ = Image::decode(&out);
            }
        }
    }
}
