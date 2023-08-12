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
            .find(|&e| e.tag == TIFFTag::GeoKeyDirectoryTag).and_then(|x|x.value.as_shorts())
            .map(|values| {
                let mut values=values.iter().cloned().array_chunks::<4>();
                // If this unwrap fails then somethings very wrong
                let directory_header=values.next().unwrap();
                let _directory_version = directory_header[0];
                let _revision = directory_header[1];
                let _minor_revision = directory_header[2];
                let number_of_keys = directory_header[3] as usize;
			                let tags= values.clone().take(number_of_keys);
				let _shorts_array:Vec<_>=values.skip(number_of_keys).flatten().collect();
                tags.filter_map(|[id, location, count, value]| {
                        // Assume no extra values are needed for now, aka location=0 and count =1
                        if location!= 0 && count != 1 {
                            eprintln!("Cannot yet handle geotiffs with non-integer valued keys, id={}, location={}, count={}",id, location, count);
                            return None;
                        };
                        Some(GeoKey::new(id,value))
                    })
                    .collect::<Vec<_>>()
            })
            .ok_or(Error::new(ErrorKind::InvalidData, "geo key directory not found."))
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
    pub value: TaggedData,
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
            .and_then(|x| x.value.as_unsigned_ints())
            .and_then(|x| x.first().copied())
            .ok_or(Error::new(
                ErrorKind::InvalidData,
                "Image length not found.",
            ))
    }

    pub fn get_image_width(&self) -> Result<usize> {
        self.entries
            .iter()
            .find(|&e| e.tag == TIFFTag::ImageWidthTag)
            .and_then(|tag| tag.value.as_unsigned_ints())
            .and_then(|x| x.first().copied())
            .ok_or(Error::new(ErrorKind::InvalidData, "Image width not found."))
    }

    pub fn get_bytes_per_sample(&self) -> Result<usize> {
        self.entries
            .iter()
            .find(|&e| e.tag == TIFFTag::BitsPerSampleTag)
            .and_then(|tag| tag.value.as_unsigned_ints())
            .and_then(|x| x.first().copied())
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

#[derive(Debug)]
pub enum GeoKey {
    GTModelTypeGeoKey(u16),
    GTRasterTypeGeoKey(u16),
    GeographicTypeGeoKey(u16),
    GeogGeodeticDatumGeoKey(u16),
    GeogPrimeMeridianGeoKey(u16),
    GeogLinearUnitsGeoKey(u16),
    GeogLinearUnitSizeGeoKey(u16),
    GeogAngularUnitsGeoKey(u16),
    GeogAngularUnitSizeGeoKey(u16),
    GeogEllipsoidGeoKey(u16),
    GeogSemiMajorAxisGeoKey(u16),
    GeogSemiMinorAxisGeoKey(u16),
    GeogInvFlatteningGeoKey(u16),
    GeogAzimuthUnitsGeoKey(u16),
    GeogPrimeMeridianLongGeoKey(u16),
    Unknown(u16, u16),
}

impl GeoKey {
    fn new(id: u16, value: u16) -> GeoKey {
        match id {
            1024 => GeoKey::GTModelTypeGeoKey(value),
            1025 => GeoKey::GTRasterTypeGeoKey(value),
            2048 => GeoKey::GeographicTypeGeoKey(value),
            2050 => GeoKey::GeogGeodeticDatumGeoKey(value),
            2051 => GeoKey::GeogPrimeMeridianGeoKey(value),
            2052 => GeoKey::GeogLinearUnitsGeoKey(value),
            2053 => GeoKey::GeogLinearUnitSizeGeoKey(value),
            2054 => GeoKey::GeogAngularUnitsGeoKey(value),
            2055 => GeoKey::GeogAngularUnitSizeGeoKey(value),
            2056 => GeoKey::GeogEllipsoidGeoKey(value),
            2057 => GeoKey::GeogSemiMajorAxisGeoKey(value),
            2058 => GeoKey::GeogSemiMinorAxisGeoKey(value),
            2059 => GeoKey::GeogInvFlatteningGeoKey(value),
            2060 => GeoKey::GeogAzimuthUnitsGeoKey(value),
            2061 => GeoKey::GeogPrimeMeridianLongGeoKey(value),
            x => GeoKey::Unknown(x, value),
        }
    }
}
