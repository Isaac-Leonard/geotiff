use enum_primitive::FromPrimitive;
use lowlevel::*;
use std::collections::HashSet;
use std::io::{Error, ErrorKind, Result};

/// The basic TIFF struct. This includes the header (specifying byte order and IFD offsets) as
/// well as all the image file directories (IFDs) plus image data.
///
/// The image data has a size of width * length * bytes_per_sample.
#[derive(Debug)]
pub struct TIFF {
    pub ifds: Vec<IFD>,
    // This is width * length * bytes_per_sample.
    pub image_data: Vec<Vec<Vec<usize>>>,
    pub geo_keys: GeoKeys,
}

/// The header of a TIFF file. This comes first in any TIFF file and contains the byte order
/// as well as the offset to the IFD table.
#[derive(Debug)]
pub struct TIFFHeader {
    pub byte_order: TIFFByteOrder,
    pub ifd_offset: Long,
}

/// An image file directory (IFD) within this TIFF. It contains the number of individual IFD entries
/// as well as a Vec with all the entries.
#[derive(Debug)]
pub struct IFD {
    pub count: u16,
    pub entries: Vec<IFDEntry>,
}

impl IFD {
    pub fn get_geo_key_directory(&self) -> Result<GeoKeyDirectoryInfo> {
        self.entries
            .iter()
            .find(|&e| e.tag == TIFFTag::GeoKeyDirectoryTag)
            .map(|x| {
                eprintln!("geo key directory value length:{}", x.value.len());
                eprintln!("geo key directory values :{:?}", x.value);
                Ok(GeoKeyDirectoryInfo {
                    directory_version: x.value[0].as_short().ok_or(Error::new(
                        ErrorKind::InvalidData,
                        "key_directory_version not a short",
                    ))?,
                    revision: x.value[1].as_short().ok_or(Error::new(
                        ErrorKind::InvalidData,
                        "key_revision not a short",
                    ))?,
                    minor_revision: x.value[2].as_short().ok_or(Error::new(
                        ErrorKind::InvalidData,
                        "minor_revision not a short",
                    ))?,
                    number_of_keys: x.value[2].as_short().ok_or(Error::new(
                        ErrorKind::InvalidData,
                        "number_of_keys not a short",
                    ))?,
                })
            })
            .ok_or(Error::new(ErrorKind::InvalidData, "Image depth not found."))?
    }
}

/// A single entry within an image file directory (IDF). It consists of a tag, a type, and several
/// tag values.
#[derive(Debug)]
pub struct IFDEntry {
    pub tag: TIFFTag,
    pub tpe: TagType,
    pub count: Long,
    pub value_offset: Long,
    pub value: Vec<TagValue>,
}

/// Implementations for the IFD struct.
impl IFD {
    pub fn get_image_length(&self) -> Result<usize> {
        self.entries
            .iter()
            .find(|&e| e.tag == TIFFTag::ImageLengthTag)
            .map(extract_value_or_0)
            .ok_or(Error::new(
                ErrorKind::InvalidData,
                "Image length not found.",
            ))
    }

    pub fn get_image_width(&self) -> Result<usize> {
        self.entries
            .iter()
            .find(|&e| e.tag == TIFFTag::ImageWidthTag)
            .map(extract_value_or_0)
            .ok_or(Error::new(ErrorKind::InvalidData, "Image width not found."))
    }

    pub fn get_bytes_per_sample(&self) -> Result<usize> {
        self.entries
            .iter()
            .find(|&e| e.tag == TIFFTag::BitsPerSampleTag)
            .map(extract_value_or_0)
            // This gets bits, so need to turn into bytes
            .map(|x| x / 8)
            .ok_or(Error::new(ErrorKind::InvalidData, "Image depth not found."))
    }
}

#[derive(Clone, Debug)]
pub struct GeoKeyDirectoryInfo {
    pub directory_version: u16,
    pub revision: u16,
    pub minor_revision: u16,
    pub number_of_keys: u16,
}
/// Decodes an u16 value into a TIFFTag.
pub fn decode_tag(value: u16) -> Option<TIFFTag> {
    TIFFTag::from_u16(value)
}

/// Decodes an u16 value into a TagType.
pub fn decode_tag_type(tpe: u16) -> Option<TagType> {
    TagType::from_u16(tpe)
}

/// Validation functions to make sure all the required tags are existing for a certain GeoTiff
/// image type (e.g., grayscale or RGB image).
pub fn validate_required_tags_for(typ: &ImageType) -> Option<HashSet<TIFFTag>> {
    let required_grayscale_tags: HashSet<TIFFTag> = [
        TIFFTag::ImageWidthTag,
        TIFFTag::ImageLengthTag,
        TIFFTag::BitsPerSampleTag,
        TIFFTag::CompressionTag,
        TIFFTag::PhotometricInterpretationTag,
        TIFFTag::StripOffsetsTag,
        TIFFTag::RowsPerStripTag,
        TIFFTag::StripByteCountsTag,
        TIFFTag::XResolutionTag,
        TIFFTag::YResolutionTag,
        TIFFTag::ResolutionUnitTag,
    ]
    .iter()
    .cloned()
    .collect();

    let required_rgb_image_tags: HashSet<TIFFTag> = [
        TIFFTag::ImageWidthTag,
        TIFFTag::ImageLengthTag,
        TIFFTag::BitsPerSampleTag,
        TIFFTag::CompressionTag,
        TIFFTag::PhotometricInterpretationTag,
        TIFFTag::StripOffsetsTag,
        TIFFTag::SamplesPerPixelTag,
        TIFFTag::RowsPerStripTag,
        TIFFTag::StripByteCountsTag,
        TIFFTag::XResolutionTag,
        TIFFTag::YResolutionTag,
        TIFFTag::ResolutionUnitTag,
    ]
    .iter()
    .cloned()
    .collect();

    match *typ {
        ImageType::Bilevel => None,
        ImageType::Grayscale => None,
        ImageType::PaletteColour => None,
        ImageType::Rgb => Some(
            required_rgb_image_tags
                .difference(&required_grayscale_tags)
                .cloned()
                .collect(),
        ),
        ImageType::YCbCr => None,
    }
}

pub(crate) fn extract_value_or_0(value: &IFDEntry) -> usize {
    match value.value[0] {
        TagValue::Short(v) => v as usize,
        TagValue::Long(v) => v as usize,
        _ => 0_usize,
    }
}

#[derive(Debug)]
pub struct GeoKeys {}
