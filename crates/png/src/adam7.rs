/// One of the seven passes of Adam7 interlacing: the pixels at columns `x0 + i * dx` and rows
/// `y0 + j * dy` of the image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pass {
    pub x0: u32,
    pub y0: u32,
    pub dx: u32,
    pub dy: u32,
}

impl Pass {
    /// The passes of Adam7, in the order they are stored.
    pub const ADAM7: [Pass; 7] = [
        Pass { x0: 0, y0: 0, dx: 8, dy: 8 },
        Pass { x0: 4, y0: 0, dx: 8, dy: 8 },
        Pass { x0: 0, y0: 4, dx: 4, dy: 8 },
        Pass { x0: 2, y0: 0, dx: 4, dy: 4 },
        Pass { x0: 0, y0: 2, dx: 2, dy: 4 },
        Pass { x0: 1, y0: 0, dx: 2, dy: 2 },
        Pass { x0: 0, y0: 1, dx: 1, dy: 2 },
    ];

    /// The width and height of the reduced image of this pass for an image of `width` by
    /// `height`; either is 0 for an empty pass, which stores no scanlines at all.
    pub fn size(&self, width: u32, height: u32) -> (u32, u32) {
        let count = |length: u32, start: u32, step: u32| if length > start { (length - start).div_ceil(step) } else { 0 };
        (count(width, self.x0, self.dx), count(height, self.y0, self.dy))
    }
}
