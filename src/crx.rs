use crate::ext::*;
use anyhow::Result;
use std::{
    io::{Read, Seek, Write},
    path::Path,
};

const MAGIC: i32 = 0x47585243; // "CRXG" in ASCII (little-endian)

#[derive(Debug)]
struct Clip {
    pub field_0: i32,
    pub field_4: i16,
    pub field_6: i16,
    pub field_8: i16,
    pub field_a: i16,
    pub field_c: i16,
    pub field_e: i16,
}

pub struct Crx {
    inner_x: i16,
    inner_y: i16,
    width: i16,
    height: i16,
    version: i16,
    flags: i16,
    bpp: i16,
    unknown: i16,
    data: Vec<u8>,
    compressed_data: Vec<u8>,
    clips: Vec<Clip>,
}

impl Crx {
    pub fn read_from_file<F: AsRef<Path> + ?Sized>(filename: &F) -> Result<Self> {
        let file = std::fs::File::open(filename)?;
        let mut file = std::io::BufReader::new(file);
        Self::read_from(&mut file, || Ok(std::fs::metadata(filename)?.len()))
    }

    pub fn read_from<R, T>(file: &mut R, stream_len: T) -> Result<Self>
    where
        R: Read + Seek,
        T: FnOnce() -> Result<u64>,
    {
        let magic = file.read_i32()?;
        if magic != MAGIC {
            return Err(anyhow::anyhow!("Invalid CRX file magic number"));
        }
        let inner_x = file.read_i16()?;
        let inner_y = file.read_i16()?;
        let width = file.read_i16()?;
        let height = file.read_i16()?;
        let version = file.read_i16()?;
        let flags = file.read_i16()?;
        let bpp = file.read_i16()?;
        let unknown = file.read_i16()?;
        if version != 2 && version != 3 {
            return Err(anyhow::anyhow!("Unsupported CRX version: {}", version));
        }
        if (flags & 0xF) > 1 {
            return Err(anyhow::anyhow!("Unsupported CRX flags: 0x{:02X}", flags));
        }
        if bpp != 0 && bpp != 1 {
            return Err(anyhow::anyhow!("Unsupported CRX bpp: {}", bpp));
        }
        let mut clips = Vec::new();
        if version >= 3 {
            let clip_count = file.read_i32()?;
            for _ in 0..clip_count {
                let field_0 = file.read_i32()?;
                let field_4 = file.read_i16()?;
                let field_6 = file.read_i16()?;
                let field_8 = file.read_i16()?;
                let field_a = file.read_i16()?;
                let field_c = file.read_i16()?;
                let field_e = file.read_i16()?;
                clips.push(Clip {
                    field_0,
                    field_4,
                    field_6,
                    field_8,
                    field_a,
                    field_c,
                    field_e,
                });
            }
        }
        let comp_size = if (flags & 0x10) == 0 {
            let size = stream_len()?;
            (size - file.stream_position()?) as u32
        } else {
            file.read_i32()? as u32
        };
        let mut compressed_data = Vec::with_capacity(comp_size as usize);
        compressed_data.resize(comp_size as usize, 0);
        file.read_exact(&mut compressed_data)?;
        let adata = fdeflate::decompress_to_vec(&compressed_data)
            .map_err(|e| anyhow::anyhow!("Failed to decompress CRX data: {:?}", e))?;
        let pixel_size = if bpp == 0 { 3 } else { 4 };
        eprintln!("Image pixel size: {}", pixel_size);
        let size = width as usize * height as usize * pixel_size as usize;
        let mut data = Vec::with_capacity(size);
        data.resize(size, 0);
        Self::decode_image(&mut data, &adata, width, height, pixel_size)?;
        Ok(Crx {
            inner_x,
            inner_y,
            width,
            height,
            version,
            flags,
            bpp,
            unknown,
            data,
            compressed_data,
            clips,
        })
    }

    pub fn export_png<F: AsRef<Path> + ?Sized>(&self, filename: &F) -> Result<()> {
        let f = std::fs::File::create(filename)?;
        let f = std::io::BufWriter::new(f);
        let mut encoder = png::Encoder::new(f, self.width as u32, self.height as u32);
        encoder.set_color(if self.bpp == 0 {
            png::ColorType::Rgb
        } else {
            png::ColorType::Rgba
        });
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&self.data)?;
        Ok(())
    }

    pub fn write_to_file<F: AsRef<Path> + ?Sized>(&self, filename: &F) -> Result<()> {
        let f = std::fs::File::create(filename)?;
        let mut f = std::io::BufWriter::new(f);
        self.write_to(&mut f)
    }

    pub fn write_to<W: Write>(&self, f: &mut W) -> Result<()> {
        f.write_i32(MAGIC)?;
        f.write_i16(self.inner_x)?;
        f.write_i16(self.inner_y)?;
        f.write_i16(self.width)?;
        f.write_i16(self.height)?;
        f.write_i16(self.version)?;
        f.write_i16(self.flags)?;
        f.write_i16(self.bpp)?;
        f.write_i16(self.unknown)?;
        if self.version >= 3 {
            f.write_i32(self.clips.len() as i32)?;
            for clip in &self.clips {
                f.write_i32(clip.field_0)?;
                f.write_i16(clip.field_4)?;
                f.write_i16(clip.field_6)?;
                f.write_i16(clip.field_8)?;
                f.write_i16(clip.field_a)?;
                f.write_i16(clip.field_c)?;
                f.write_i16(clip.field_e)?;
            }
        }
        if (self.flags & 0x10) != 0 {
            f.write_i32(self.compressed_data.len() as i32)?;
        }
        f.write_all(&self.compressed_data)?;
        Ok(())
    }

    fn decode_row0(
        dst: &mut Vec<u8>,
        mut dst_p: usize,
        src: &[u8],
        mut src_p: usize,
        width: i16,
        pixel_size: i8,
    ) -> Result<usize> {
        let mut prev_p = dst_p;
        for _ in 0..pixel_size {
            dst[dst_p] = src[src_p];
            dst_p += 1;
            src_p += 1;
        }
        let remaining = width - 1;
        for _ in 0..remaining {
            for _ in 0..pixel_size {
                dst[dst_p] = src[src_p].overflowing_add(dst[prev_p]).0;
                dst_p += 1;
                src_p += 1;
                prev_p += 1;
            }
        }
        Ok(src_p)
    }

    fn decode_row1(
        dst: &mut Vec<u8>,
        mut dst_p: usize,
        src: &[u8],
        mut src_p: usize,
        width: i16,
        pixel_size: i8,
        mut prev_row_p: usize,
    ) -> Result<usize> {
        for _ in 0..width {
            for _ in 0..pixel_size {
                dst[dst_p] = src[src_p].overflowing_add(dst[prev_row_p]).0;
                dst_p += 1;
                src_p += 1;
                prev_row_p += 1;
            }
        }
        Ok(src_p)
    }

    fn decode_row2(
        dst: &mut Vec<u8>,
        mut dst_p: usize,
        src: &[u8],
        mut src_p: usize,
        width: i16,
        pixel_size: i8,
        mut prev_row_p: usize,
    ) -> Result<usize> {
        for _ in 0..pixel_size {
            dst[dst_p] = src[src_p];
            dst_p += 1;
            src_p += 1;
        }
        let remaining = width - 1;
        for _ in 0..remaining {
            for _ in 0..pixel_size {
                dst[dst_p] = src[src_p].overflowing_add(dst[prev_row_p]).0;
                dst_p += 1;
                src_p += 1;
                prev_row_p += 1;
            }
        }
        Ok(src_p)
    }

    fn decode_row3(
        dst: &mut Vec<u8>,
        mut dst_p: usize,
        src: &[u8],
        mut src_p: usize,
        width: i16,
        pixel_size: i8,
        mut prev_row_p: usize,
    ) -> Result<usize> {
        let count = width - 1;
        prev_row_p += pixel_size as usize;
        for _ in 0..count {
            for _ in 0..pixel_size {
                dst[dst_p] = src[src_p].overflowing_add(dst[prev_row_p]).0;
                dst_p += 1;
                src_p += 1;
                prev_row_p += 1;
            }
        }
        for _ in 0..pixel_size {
            dst[dst_p] = src[src_p];
            dst_p += 1;
            src_p += 1;
        }
        Ok(src_p)
    }

    fn decode_row4(
        dst: &mut Vec<u8>,
        dst_p: usize,
        src: &[u8],
        mut src_p: usize,
        width: i16,
        pixel_size: i8,
    ) -> Result<usize> {
        for offset in 0..pixel_size {
            let mut dst_c = dst_p + offset as usize;
            let mut remaining = width;
            let value = src[src_p];
            src_p += 1;
            dst[dst_c] = value;
            dst_c += pixel_size as usize;
            remaining -= 1;
            if remaining == 0 {
                continue;
            }
            if value == src[src_p] {
                src_p += 1;
                let count = src[src_p] as i16;
                src_p += 1;
                remaining -= count;
                for _ in 0..count {
                    dst[dst_c] = value;
                    dst_c += pixel_size as usize;
                }
            }
            while remaining > 0 {
                let value = src[src_p];
                src_p += 1;
                dst[dst_c] = value;
                dst_c += pixel_size as usize;
                remaining -= 1;
                if remaining == 0 {
                    break;
                }
                if value == src[src_p] {
                    src_p += 1;
                    let count = src[src_p] as i16;
                    src_p += 1;
                    remaining -= count;
                    for _ in 0..count {
                        dst[dst_c] = value;
                        dst_c += pixel_size as usize;
                    }
                }
            }
        }
        Ok(src_p)
    }

    fn decode_image(
        dst: &mut Vec<u8>,
        src: &[u8],
        width: i16,
        height: i16,
        pixel_size: i8,
    ) -> Result<()> {
        let mut src_p = 0;
        let mut dst_p = 0;
        let mut prev_row_p = 0;
        for _ in 0..height {
            let data = src[src_p];
            src_p += 1;
            match data {
                0 => {
                    src_p = Self::decode_row0(dst, dst_p, src, src_p, width, pixel_size)?;
                }
                1 => {
                    src_p =
                        Self::decode_row1(dst, dst_p, src, src_p, width, pixel_size, prev_row_p)?;
                }
                2 => {
                    src_p =
                        Self::decode_row2(dst, dst_p, src, src_p, width, pixel_size, prev_row_p)?;
                }
                3 => {
                    src_p =
                        Self::decode_row3(dst, dst_p, src, src_p, width, pixel_size, prev_row_p)?;
                }
                4 => {
                    src_p = Self::decode_row4(dst, dst_p, src, src_p, width, pixel_size)?;
                }
                _ => {
                    return Err(anyhow::anyhow!("Invalid row type: {}", data));
                }
            }
            prev_row_p = dst_p;
            dst_p += pixel_size as usize * width as usize;
        }
        if pixel_size == 4 {
            for p in (0..dst.len()).step_by(4) {
                let a = dst[p + 0];
                let b = dst[p + 1];
                let g = dst[p + 2];
                let r = dst[p + 3];
                dst[p + 0] = r;
                dst[p + 1] = g;
                dst[p + 2] = b;
                dst[p + 3] = 0xff - a;
            }
        }
        Ok(())
    }
}
