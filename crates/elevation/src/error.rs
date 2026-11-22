use png::PngError;
use std::fmt;
use std::io;

/// Why tiles could not be decoded or loaded.
#[derive(Debug)]
pub enum ElevationError {
    Io(io::Error),
    /// The tile is not a valid PNG image.
    Png(PngError),
    /// The tile does not fit: coordinates outside its zoom, a zoom other than the [`Dem`](crate::Dem)'s,
    /// a size other than the other tiles', or heights that do not match its size.
    InvalidTile(&'static str),
}

impl fmt::Display for ElevationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ElevationError::Io(e) => write!(f, "{e}"),
            ElevationError::Png(e) => write!(f, "{e}"),
            ElevationError::InvalidTile(what) => write!(f, "invalid tile: {what}"),
        }
    }
}

impl std::error::Error for ElevationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ElevationError::Io(e) => Some(e),
            ElevationError::Png(e) => Some(e),
            ElevationError::InvalidTile(_) => None,
        }
    }
}

impl From<io::Error> for ElevationError {
    fn from(e: io::Error) -> Self {
        ElevationError::Io(e)
    }
}

impl From<PngError> for ElevationError {
    fn from(e: PngError) -> Self {
        ElevationError::Png(e)
    }
}
