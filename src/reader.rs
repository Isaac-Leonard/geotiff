use num::FromPrimitive;
use std::fs::File;
use std::io::{Error, ErrorKind, Read, Result, Seek, SeekFrom};
use std::path::Path;

use byteorder::{BigEndian, ByteOrder, LittleEndian, ReadBytesExt};

use lowlevel::{tag_size, TIFFByteOrder, TIFFTag, TagType};
use tiff::{decode_tag, decode_tag_type, IFDEntry, IFD, TIFF};

use crate::lowlevel::TaggedData;

/// A helper trait to indicate that something needs to be seekable and readable.
pub trait SeekableReader: Seek + Read {}

impl<T: Seek + Read> SeekableReader for T {}

/// The TIFF reader class that encapsulates all functionality related to reading `.tiff` files.
/// In particular, this includes reading the TIFF header, the image file directories (IDF), and
/// the plain data.
pub struct TIFFReader;

impl TIFFReader {
    /// Loads a `.tiff` file, as specified by `filename`.
    pub fn load<T: AsRef<Path>>(&self, path: T) -> Result<Box<TIFF>> {
        let mut reader = File::open(path)?;

        self.read(&mut reader)
    }

    /// Reads the `.tiff` file, starting with the byte order.
    pub fn read(&self, reader: &mut dyn SeekableReader) -> Result<Box<TIFF>> {
        match self.read_byte_order(reader)? {
            TIFFByteOrder::LittleEndian => self.read_tiff::<LittleEndian>(reader),
            TIFFByteOrder::BigEndian => self.read_tiff::<BigEndian>(reader),
        }
    }

    /// Helper function to read the byte order, one of `LittleEndian` or `BigEndian`.
    pub fn read_byte_order(&self, reader: &mut dyn SeekableReader) -> Result<TIFFByteOrder> {
        // Bytes 0-1: "II" or "MM"
        // Read and validate ByteOrder
        match TIFFByteOrder::from_u16(reader.read_u16::<LittleEndian>()?) {
            Some(TIFFByteOrder::LittleEndian) => Ok(TIFFByteOrder::LittleEndian),
            Some(TIFFByteOrder::BigEndian) => Ok(TIFFByteOrder::BigEndian),
            None => Err(Error::new(
                ErrorKind::Other,
                "Invalid byte order in header.".to_string(),
            )),
        }
    }

    /// Reads the `.tiff` file, given a `ByteOrder`.
    ///
    /// This starts by reading the magic number, the IFD offset, the IFDs themselves, and finally,
    /// the image data.
    fn read_tiff<T: ByteOrder>(&self, reader: &mut dyn SeekableReader) -> Result<Box<TIFF>> {
        self.read_magic::<T>(reader)?;
        let ifd_offset = self.read_ifd_offset::<T>(reader)?;
        let ifd = self.read_IFD::<T>(reader, ifd_offset)?;
        let image_data = self.read_image_data::<T>(reader, &ifd)?;
        Ok(Box::new(TIFF {
            ifds: vec![ifd],
            image_data,
        }))
    }

    /// Gets the geo_keys if they exist

    /// Reads the magic number, i.e., 42.
    fn read_magic<T: ByteOrder>(&self, reader: &mut dyn SeekableReader) -> Result<()> {
        // Bytes 2-3: 0042
        // Read and validate HeaderMagic
        match reader.read_u16::<T>()? {
            42 => Ok(()),
            _ => Err(Error::new(
                ErrorKind::Other,
                "Invalid magic number in header",
            )),
        }
    }

    /// Reads the IFD offset. The first IFD is then read from this position.
    pub fn read_ifd_offset<T: ByteOrder>(&self, reader: &mut dyn SeekableReader) -> Result<u32> {
        // Bytes 4-7: offset
        // Offset from start of file to first IFD
        let ifd_offset_field = reader.read_u32::<T>()?;
        Ok(ifd_offset_field)
    }

    /// Reads an IFD.
    ///
    /// This starts by reading the number of entries, and then the tags within each entry.
    #[allow(non_snake_case)]
    fn read_IFD<T: ByteOrder>(
        &self,
        reader: &mut dyn SeekableReader,
        ifd_offset: u32,
    ) -> Result<IFD> {
        reader.seek(SeekFrom::Start(ifd_offset as u64))?;
        // 2 byte count of IFD entries
        let entry_count = reader.read_u16::<T>()?;

        let mut ifd = IFD {
            count: entry_count,
            entries: Vec::with_capacity(entry_count as usize),
        };

        for entry_number in 0..entry_count as usize {
            let entry = self.read_tag::<T>(ifd_offset as u64 + 2, entry_number, reader);
            match entry {
                Ok(e) => ifd.entries.push(e),
                Err(err) => println!("Invalid tag at index {}: {}", entry_number, err),
            }
        }

        Ok(ifd)
    }

    /// Reads `n` bytes from a reader into a Vec<u8>.
    fn read_n(&self, reader: &mut dyn SeekableReader, bytes_to_read: u64) -> Vec<u8> {
        let mut buf = Vec::with_capacity(bytes_to_read as usize);
        let mut chunk = reader.take(bytes_to_read);
        let status = chunk.read_to_end(&mut buf);
        match status {
            Ok(n) => assert_eq!(bytes_to_read as usize, n),
            _ => panic!("Didn't read enough"),
        }
        buf
    }

    /// Converts a Vec<u8> into a TagValue, depending on the type of the tag. In the TIFF file
    /// format, each tag type indicates which value it stores (e.g., a byte, ascii, or long value).
    /// This means that the tag values have to be read taking the tag type into consideration.
    fn bytes_to_tag_value<Endian: ByteOrder>(&self, vec: Vec<u8>, tpe: &TagType) -> TaggedData {
        let _len = vec.len();
        match tpe {
            TagType::Byte => TaggedData::Byte(vec),
            TagType::ASCII => TaggedData::Ascii(String::from_utf8_lossy(&vec).to_string()),
            TagType::Short => {
                TaggedData::Short(vec.chunks_exact(2).map(Endian::read_u16).collect())
            }
            TagType::Long => TaggedData::Long(vec.chunks_exact(4).map(Endian::read_u32).collect()),
            TagType::Rational => TaggedData::Rational(
                vec.chunks_exact(4)
                    .array_chunks::<2>()
                    .map(|[num, den]| (Endian::read_u32(num), Endian::read_u32(den)))
                    .collect(),
            ),
            &TagType::SignedByte => TaggedData::SignedByte(vec.iter().map(|x| *x as i8).collect()),
            &TagType::SignedShort => {
                TaggedData::SignedShort(vec.chunks_exact(2).map(Endian::read_i16).collect())
            }
            &TagType::SignedLong => {
                TaggedData::SignedLong(vec.chunks_exact(4).map(Endian::read_i32).collect())
            }
            &TagType::SignedRational => TaggedData::SignedRational(
                vec.chunks_exact(4)
                    .array_chunks::<2>()
                    .map(|[num, den]| (Endian::read_i32(num), Endian::read_i32(den)))
                    .collect(),
            ),
            &TagType::Float => {
                TaggedData::Float(vec.chunks_exact(4).map(Endian::read_f32).collect())
            }
            &TagType::Double => {
                TaggedData::Double(vec.chunks_exact(8).map(Endian::read_f64).collect())
            }
            &TagType::Undefined => TaggedData::Byte(Vec::new()),
        }
    }

    /// Converts a number of u8 values to a usize value. This doesn't check if usize is at least
    /// u64, so be careful with large values.
    fn vec_to_value<Endian: ByteOrder>(&self, vec: Vec<u8>) -> usize {
        let len = vec.len();
        match len {
            0 => 0_usize,
            1 => vec[0] as usize,
            2 => Endian::read_u16(&vec[..]) as usize,
            4 => Endian::read_u32(&vec[..]) as usize,
            8 => Endian::read_u64(&vec[..]) as usize,
            n => panic!("Vector has wrong number of elements, found len={}", n),
        }
    }

    /// Reads a single tag (given an IFD offset) into an IFDEntry.
    ///
    /// This consists of reading the tag ID, field type, number of values, offset to values. After
    /// decoding the tag and type, the values are retrieved.
    fn read_tag<Endian: ByteOrder>(
        &self,
        ifd_offset: u64,
        entry_number: usize,
        reader: &mut dyn SeekableReader,
    ) -> Result<IFDEntry> {
        // Seek beginning (as each tag is 12 bytes long).
        reader.seek(SeekFrom::Start(ifd_offset + 12 * entry_number as u64))?;

        // Bytes 0..1: u16 tag ID
        let tag_value = reader.read_u16::<Endian>()?;

        // Bytes 2..3: u16 field Type
        let tpe_value = reader.read_u16::<Endian>()?;

        // Bytes 4..7: u32 number of Values of type
        let count_value = reader.read_u32::<Endian>()?;

        // Bytes 8..11: u32 offset in file to Value
        let value_offset_value = reader.read_u32::<Endian>()?;

        // Decode the tag.
        let tag_msg = format!("Invalid tag {:04X}", tag_value);
        let tag = decode_tag(tag_value).ok_or(Error::new(ErrorKind::InvalidData, tag_msg))?;

        // Decode the type.
        let tpe_msg = format!("Invalid tag type {:04X}", tpe_value);
        let tpe = decode_tag_type(tpe_value).expect(&tpe_msg);
        let value_size = tag_size(&tpe);

        // Let's get the value(s) of this tag.
        let total_size = count_value * value_size;
        /*        println!(
            "{:04X} {:04X} {:08X} {:08X} {:?} {:?} {:?} {:?}",
            tag_value, tpe_value, count_value, value_offset_value, tag, tpe, value_size, tot_size
        );*/
        let number_of_bytes_to_read = (value_size * count_value) as u64;
        let values: Vec<u8> = if total_size <= 4 {
            // Can directly read the value at the value field. For simplicity, we simply reset
            // the reader to the correct position.
            reader.seek(SeekFrom::Start(ifd_offset + 12 * entry_number as u64 + 8))?;
            self.read_n(reader, number_of_bytes_to_read)
        } else {
            // Have to read from the address pointed at by the value field.
            reader.seek(SeekFrom::Start(value_offset_value as u64))?;
            self.read_n(reader, number_of_bytes_to_read)
        };

        // Create IFD entry.
        let ifd_entry = IFDEntry {
            tag,
            count: count_value,
            value_offset: value_offset_value,
            value: self.bytes_to_tag_value::<Endian>(values, &tpe),
            tpe,
        };

        /*        println!(
            "IFD[{:?}] tag: {:?} type: {:?} count: {} offset: {:08x} value: {:?}",
            entry_number,
            ifd_entry.tag,
            ifd_entry.tpe,
            ifd_entry.count,
            ifd_entry.value_offset,
            ifd_entry.value
        );*/

        Ok(ifd_entry)
    }

    fn get_image_size_data(&self, ifd: &IFD) -> ImageSizeData {
        // Storage location with  in the TIFF. First, lets get the number of rows per strip.
        let rows_per_strip = ifd
            .get(TIFFTag::RowsPerStripTag)
            // TODO: Should maybe error here if that fails
            .and_then(|x| x.value.as_unsigned_ints())
            .and_then(|x| x.first().copied().map(|x| x as u32))
            .unwrap_or_else(u32::max_value);
        // For each strip, its offset within the TIFF file.
        let strip_offsets = ifd
            .get(TIFFTag::StripOffsetsTag)
            .and_then(|x| x.value.as_unsigned_ints());
        let strip_row_byte_counts = ifd
            .get(TIFFTag::StripByteCountsTag)
            .and_then(|x| x.value.as_unsigned_ints());
        let _plainar_configuration = ifd.get(TIFFTag::PlanarConfigurationTag);
        match strip_offsets.zip(strip_row_byte_counts) {
            Some((strip_offsets, strip_row_byte_countt)) => ImageSizeData::Image(StripImageData {
                strip_offsets,
                strip_row_byte_countt,
                rows_per_strip,
            }),
            _ => {
                let tile_width = ifd
                    .get(TIFFTag::TileWidthTag)
                    .and_then(|x| x.value.as_unsigned_ints())
                    .and_then(|x| x.first().copied())
                    .expect("Not enough tile or strip tags found");
                let tile_length = ifd
                    .get(TIFFTag::TileHeightTag)
                    .and_then(|x| x.value.as_unsigned_ints())
                    .and_then(|x| x.first().copied())
                    .expect("Not enough tile or strip tags found");
                let tile_bytes_offsets = ifd
                    .get(TIFFTag::TileOffsetsTag)
                    .expect("Not enough tile or strip tags found");
                let tile_bytes_counts = ifd
                    .get(TIFFTag::TileByteCountTag)
                    .expect("Not enough tile or strip tags found");
                ImageSizeData::Tiles(TiledImageData {
                    tile_width,
                    tile_length,
                    tile_bytes_offsets,
                    tile_bytes_counts,
                })
            }
        }
    }

    /// Reads the image data into a 3D-Vec<u8>.
    ///
    /// As for now, the following assumptions are made:
    /// * No compression is used, i.e., CompressionTag == 1.
    fn read_image_data<T: ByteOrder>(
        &self,
        reader: &mut dyn SeekableReader,
        ifd: &IFD,
    ) -> Result<Vec<Vec<Vec<usize>>>> {
        let image_size_data = self.get_image_size_data(ifd);
        match image_size_data {
            ImageSizeData::Tiles(specifications) => {
                self.read_tiled_image::<T>(reader, ifd, specifications)
            }
            ImageSizeData::Image(specifications) => {
                self.read_strip_image::<T>(reader, ifd, specifications)
            }
        }
    }

    fn read_strip_image<Endian: ByteOrder>(
        &self,
        reader: &mut dyn SeekableReader,
        ifd: &IFD,
        specifications: StripImageData,
    ) -> Result<Vec<Vec<Vec<usize>>>> {
        let StripImageData {
            rows_per_strip: _,
            strip_offsets,
            strip_row_byte_countt: strip_row_byte_counts,
        } = specifications;
        // Image size and depth.
        let image_length = ifd.get_image_length()?;
        let image_width = ifd.get_image_width()?;
        let image_depth = ifd.get_bytes_per_sample()?;
        // Create the output Vec.

        // TODO The img Vec should optimally not be of usize, but of size "image_depth".
        let mut img: Vec<Vec<Vec<usize>>> = Vec::with_capacity(image_length);
        for i in 0..image_length {
            img.push(Vec::with_capacity(image_width));
            for _j in 0..image_width {
                img[i].push(Vec::with_capacity(image_depth));
            }
        }

        // Read strip after strip, and copy it into the output Vec.
        let offsets = strip_offsets.clone();
        let byte_counts = strip_row_byte_counts;
        // A bit much boilerplate, but should be okay and fast.
        let mut curr_x = 0;
        let mut curr_y = 0;
        let mut curr_z = 0;
        for (offset, byte_count) in offsets.iter().zip(byte_counts.iter()) {
            reader.seek(SeekFrom::Start(*offset as u64))?;
            for _i in 0..(*byte_count / image_depth) {
                let v = self.read_n(reader, image_depth as u64);
                img[curr_x][curr_y].push(self.vec_to_value::<Endian>(v));
                curr_z += 1;
                if curr_z >= img[curr_x][curr_y].len() {
                    curr_z = 0;
                    curr_y += 1;
                }
                if curr_y >= img[curr_x].len() {
                    curr_y = 0;
                    curr_x += 1;
                }
            }
        }

        // Return the output Vec.
        Ok(img)
    }

    fn read_tiled_image<Endian: ByteOrder>(
        &self,
        reader: &mut dyn SeekableReader,
        ifd: &IFD,
        specifications: TiledImageData,
    ) -> Result<Vec<Vec<Vec<usize>>>> {
        let TiledImageData {
            tile_width,
            tile_length,
            tile_bytes_offsets,
            tile_bytes_counts,
        } = specifications;
        // Image size and depth.
        let image_length = ifd.get_image_length()?;
        let image_width = ifd.get_image_width()?;
        let image_depth = ifd.get_bytes_per_sample()?;
        // Create the output Vec.

        // TODO The img Vec should optimally not be of usize, but of size "image_depth".
        let mut img: Vec<Vec<Vec<usize>>> = Vec::with_capacity(image_length);
        for i in 0..image_length {
            img.push(Vec::with_capacity(image_width));
            for _j in 0..image_width {
                img[i].push(Vec::with_capacity(image_depth));
            }
        }

        // Read tile after tile, and copy it into the output Vec.
        // Unwrap here, we know it has to be a long or short or if not somethings very wrong
        // Error handling code would just be removed when we fix how individual tag values are represented
        let offsets = tile_bytes_offsets
            .value
            .as_unsigned_ints()
            .ok_or(Error::new(ErrorKind::InvalidData, "Couldn't read offsets"))?;
        let byte_counts = tile_bytes_counts
            .value
            .as_unsigned_ints()
            .ok_or(Error::new(
                ErrorKind::InvalidData,
                "Couldn't read byte counts",
            ))?;
        // A bit much boilerplate, but should be okay and fast.
        let mut curr_z = 0;
        let tiles_across = (image_width + tile_width - 1) / tile_width;
        let tiles_down = (image_length + tile_length - 1) / tile_length;
        for (nth_tile, (offset, byte_count)) in offsets.iter().zip(byte_counts.iter()).enumerate() {
            let tile_col = nth_tile % tiles_across;
            let tile_row = nth_tile / tiles_across;
            let start_x = tile_col * tile_width;
            let mut curr_x = start_x;
            let end_x = (tile_col + 1) * tile_width;
            let max_y = tiles_down * tile_length;
            let start_y = max_y - (tile_row + 1) * tile_length;
            let mut curr_y = start_y;
            let _end_y = max_y - tile_row * tile_length;
            reader.seek(SeekFrom::Start(*offset as u64))?;
            for _i in 0..(*byte_count / image_depth) {
                let v = self.read_n(reader, image_depth as u64);
                if curr_x >= image_width || curr_y >= image_length {
                    curr_z += 1;
                    if curr_z >= img[0][0].len() {
                        curr_z = 0;
                        curr_x += 1;
                    }
                    if curr_x >= end_x {
                        curr_x = start_x;
                        curr_y += 1;
                    }
                    continue;
                }
                img[curr_y][curr_x].push(self.vec_to_value::<Endian>(v));
                curr_z += 1;
                if curr_z >= img[curr_y][curr_x].len() {
                    curr_z = 0;
                    curr_x += 1;
                }
                if curr_x >= end_x {
                    curr_x = start_x;
                    curr_y += 1;
                }
            }
        }

        // Return the output Vec.
        Ok(img)
    }
}

enum ImageSizeData {
    Tiles(TiledImageData),
    Image(StripImageData),
}

struct StripImageData {
    strip_offsets: Vec<usize>,
    strip_row_byte_countt: Vec<usize>,
    rows_per_strip: u32,
}

#[derive(Debug)]
struct TiledImageData {
    tile_width: usize,
    tile_length: usize,
    tile_bytes_counts: IFDEntry,
    tile_bytes_offsets: IFDEntry,
}
