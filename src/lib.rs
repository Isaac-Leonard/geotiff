#![feature(iter_array_chunks)]

extern crate byteorder;
#[macro_use]
extern crate enum_primitive;
extern crate num;

use std::fmt;
use std::io::Result;

use std::path::Path;

mod lowlevel;
mod reader;
pub mod tiff;

use reader::*;
pub use tiff::TIFF;

/// The GeoTIFF library reads `.tiff` files.
///
/// It is primarily used within a routing application that needs to parse digital elevation models.
/// As such, other use cases are NOT tested (for now).
impl TIFF {
    /// Opens a `.tiff` file at the location indicated by `filename`.
    pub fn open<T: AsRef<Path>>(path: T) -> Result<Box<TIFF>> {
        let tiff_reader = TIFFReader;
        tiff_reader.load(path)
    }

    /// Gets the value at a given coordinate (in pixels).
    pub fn get_value_at(&self, lon: usize, lat: usize) -> usize {
        self.image_data[lon][lat][0]
    }
}

/// Overwrite default display function.
impl fmt::Display for TIFF {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "TIFF(Image size: [{}, {}, {}], Tag data: {:?})",
            self.image_data.len(),
            self.image_data[0].len(),
            self.image_data[0][0].len(),
            self.ifds
        )
    }
}
