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
    pub fn get_geo_keys(&self) -> Result<Vec<GeoKey>> {
        self.entries
            .iter()
            .find(|&e| e.tag == TIFFTag::GeoKeyDirectoryTag)
            .map(|x| {
                let _directory_version = x.value[0].as_short().ok_or(Error::new(
                    ErrorKind::InvalidData,
                    "key_directory_version not a short",
                ))?;
                let _revision = x.value[1].as_short().ok_or(Error::new(
                    ErrorKind::InvalidData,
                    "key_revision not a short",
                ))?;
                let _minor_revision = x.value[2].as_short().ok_or(Error::new(
                    ErrorKind::InvalidData,
                    "minor_revision not a short",
                ))?;
                let number_of_keys = x.value[3].as_short().ok_or(Error::new(
                    ErrorKind::InvalidData,
                    "number_of_keys not a short",
                ))?;

                x.value
                    .iter()
                    .skip(4)
                    .take(number_of_keys as usize * 4)
                    .array_chunks::<4>()
                    .map(|[id, location, count, val_or_offset]| {
                        println!("parsing key");
                        // Assume no extra values are needed for now, aka location=0 and count =1
                        if location.as_unsigned_int()? != 0 && count.as_unsigned_int()? != 1 {
                            panic!("Cannot yet handle geotiffs with non-integer valued keys, id={}, location={}, count={}",id.as_unsigned_int()?, location.as_unsigned_int()? != 0 ,count.as_unsigned_int()?)
                        };
                        let id = id.as_short()?;
                        let value = val_or_offset.as_short()?;
                        Some(match id {
                            1024 => GeoKey::GTModelTypeGeoKey(value),
                            1025 => GeoKey::GTRasterTypeGeoKey(value),
                            2048 => GeoKey::GeographicTypeGeoKey(value),
                            2050 => GeoKey::GeogGeodeticDatumGeoKey(value),
                            2051 => GeoKey::GeogPrimeMeridianGeoKey(value),
                            2052 => GeoKey::GeogLinearUnitsGeoKey(value),
                            2053 => GeoKey::GeogLinearUnitSizeGeoKey(value),
                            2054 => GeoKey::GeogAngularUnitsGeoKey(value),
                            x => GeoKey::Unknown(x, value),
                        })
                    })
                    .collect::<Option<Vec<_>>>()
                    .ok_or(Error::new(
                        ErrorKind::InvalidData,
                        "Could not parse geo keys properly",
                    ))
            })
            .ok_or(Error::new(ErrorKind::InvalidData, "Image depth not found."))?
    }
}

/// A single entry within an image file directory (IDF). It consists of a tag, a type, and several
/// tag values.
#[derive(Debug, Clone)]
pub struct IFDEntry {
    pub tag: TIFFTag,
    pub tpe: TagType,
    pub count: Long,
    pub value_offset: Long,
    pub value: Vec<TagValue>,
}

/// Implementations for the IFD struct.
impl IFD {
    pub fn get(&self, tag: TIFFTag) -> Option<IFDEntry> {
        self.entries.iter().find(|&e| e.tag == tag).cloned()
    }

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
pub enum GeoKey {
    GTModelTypeGeoKey(u16),
    GTRasterTypeGeoKey(u16),
    GeographicTypeGeoKey(u16),
    GeogLinearUnitsGeoKey(u16),
    GeogAngularUnitsGeoKey(u16),
    GeogGeodeticDatumGeoKey(u16),
    GeogPrimeMeridianGeoKey(u16),
    GeogLinearUnitSizeGeoKey(u16),
    Unknown(u16, u16),
}
