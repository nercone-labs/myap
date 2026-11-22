/// The CRC-32 of ISO 3309 and ITU-T V.42 (polynomial `0xEDB88320` in reflected form), as PNG
/// chunks and gzip use it: initialised to all ones and inverted at the end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Crc32 {
    state: u32,
}

impl Crc32 {
    /// The polynomial, reflected (bit 0 is the coefficient of `x^31`).
    pub const POLYNOMIAL: u32 = 0xEDB8_8320;

    /// The table of the CRC of every byte value, computed at compile time.
    const TABLE: [u32; 256] = Self::table();

    const fn table() -> [u32; 256] {
        let mut table = [0u32; 256];
        let mut n = 0;
        while n < 256 {
            let mut c = n as u32;
            let mut k = 0;
            while k < 8 {
                c = if c & 1 != 0 { Self::POLYNOMIAL ^ (c >> 1) } else { c >> 1 };
                k += 1;
            }
            table[n] = c;
            n += 1;
        }
        table
    }

    /// A CRC over no bytes yet.
    pub const fn new() -> Crc32 {
        Crc32 { state: 0xFFFF_FFFF }
    }

    /// Adds `bytes` to the CRC.
    pub fn update(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.state = Self::TABLE[((self.state ^ u32::from(byte)) & 0xFF) as usize] ^ (self.state >> 8);
        }
    }

    /// The CRC of the bytes added so far.
    pub const fn value(&self) -> u32 {
        self.state ^ 0xFFFF_FFFF
    }

    /// The CRC of `bytes`.
    pub fn of(bytes: &[u8]) -> u32 {
        let mut crc = Crc32::new();
        crc.update(bytes);
        crc.value()
    }
}

impl Default for Crc32 {
    fn default() -> Self {
        Crc32::new()
    }
}
