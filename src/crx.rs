use crate::{ext::*, utils};
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
    encode_type: Vec<u8>,
}

impl std::fmt::Debug for Crx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Crx")
            .field("inner_x", &self.inner_x)
            .field("inner_y", &self.inner_y)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("version", &self.version)
            .field("flags", &self.flags)
            .field("bpp", &self.bpp)
            .field("unknown", &self.unknown)
            .field("data_size", &self.data.len())
            .field("compressed_data_size", &self.compressed_data.len())
            .field("clips", &self.clips)
            .finish()
    }
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
        let adata = if compressed_data.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
            crate::utils::decompress_data(&compressed_data)?
        } else {
            fdeflate::decompress_to_vec(&compressed_data)
                .map_err(|e| anyhow::anyhow!("Failed to decompress CRX data: {:?}", e))?
        };
        let pixel_size = if bpp == 0 { 3 } else { 4 };
        let size = width as usize * height as usize * pixel_size as usize;
        let mut data = Vec::with_capacity(size);
        data.resize(size, 0);
        let mut encode_type = Vec::with_capacity(height as usize);
        Self::decode_image(
            &mut data,
            &adata,
            width,
            height,
            pixel_size,
            &mut encode_type,
        )?;
        let crx = Crx {
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
            encode_type,
        };
        eprintln!("Image metadata: {:?}", crx);
        Ok(crx)
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

    pub fn import_png<F: AsRef<Path> + ?Sized>(&mut self, filename: &F) -> Result<()> {
        let f = std::fs::File::open(filename)?;
        let mut decoder = png::Decoder::new(f);
        let info = decoder.read_header_info()?;
        if info.width != self.width as u32 {
            return Err(anyhow::anyhow!(
                "Image width mismatch: expected {}, got {}",
                self.width,
                info.width
            ));
        }
        if info.height != self.height as u32 {
            return Err(anyhow::anyhow!(
                "Image height mismatch: expected {}, got {}",
                self.height,
                info.height
            ));
        }
        if info.bit_depth != png::BitDepth::Eight {
            return Err(anyhow::anyhow!(
                "Image bit depth mismatch: expected 8, got {:?}",
                info.bit_depth
            ));
        }
        if info.color_type != png::ColorType::Rgb && info.color_type != png::ColorType::Rgba {
            return Err(anyhow::anyhow!(
                "Image color type mismatch: expected RGB or RGBA, got {:?}",
                info.color_type
            ));
        }
        let ct = info.color_type;
        let mut reader = decoder.read_info()?;
        let size = self.width as usize
            * self.height as usize
            * if ct == png::ColorType::Rgb { 3 } else { 4 };
        let mut data = Vec::with_capacity(size);
        data.resize(size, 0);
        reader.next_frame(&mut data)?;
        let data = if self.bpp == 0 && ct == png::ColorType::Rgba {
            Self::rgba_to_rgb(&data)
        } else if self.bpp == 1 && ct == png::ColorType::Rgb {
            Self::rgb_to_rgba(&data)
        } else {
            data
        };
        let edata = if self.bpp == 0 {
            Self::encode_image_bbp24(&data, self.width, self.height, &self.encode_type)?
        } else {
            Self::encode_image_bbp32(&data, self.width, self.height, &self.encode_type)?
        };
        let compressed_data = utils::compress_data(&edata)?;
        self.data = data;
        self.compressed_data = compressed_data;
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
        f.write_i16(self.flags | 0x10)?;
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
        f.write_i32(self.compressed_data.len() as i32)?;
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
        encode_type: &mut Vec<u8>,
    ) -> Result<()> {
        let mut src_p = 0;
        let mut dst_p = 0;
        let mut prev_row_p = 0;
        for _ in 0..height {
            let data = src[src_p];
            encode_type.push(data);
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
        } else if pixel_size == 3 {
            for p in (0..dst.len()).step_by(3) {
                let b = dst[p + 0];
                let g = dst[p + 1];
                let r = dst[p + 2];
                dst[p + 0] = r;
                dst[p + 1] = g;
                dst[p + 2] = b;
            }
        }
        Ok(())
    }

    fn encode_bbp24_row0(
        dst: &mut Vec<u8>,
        mut dst_p: usize,
        src: &[u8],
        width: i16,
        y: i16,
    ) -> Result<usize> {
        let mut src_p = y as usize * width as usize * 3;
        dst[dst_p] = src[src_p + 2];
        dst[dst_p + 1] = src[src_p + 1];
        dst[dst_p + 2] = src[src_p];
        dst_p += 3;
        src_p += 3;
        for _ in 1..width {
            dst[dst_p] = src[src_p + 2].overflowing_sub(src[src_p - 1]).0;
            dst[dst_p + 1] = src[src_p + 1].overflowing_sub(src[src_p - 2]).0;
            dst[dst_p + 2] = src[src_p].overflowing_sub(src[src_p - 3]).0;
            dst_p += 3;
            src_p += 3;
        }
        return Ok(dst_p);
    }

    fn encode_bbp24_row1(
        dst: &mut Vec<u8>,
        mut dst_p: usize,
        src: &[u8],
        width: i16,
        y: i16,
    ) -> Result<usize> {
        let mut src_p = y as usize * width as usize * 3;
        let mut prev_row_p = (y - 1) as usize * width as usize * 3;
        for _ in 0..width {
            dst[dst_p] = src[src_p + 2].overflowing_sub(src[prev_row_p + 2]).0;
            dst[dst_p + 1] = src[src_p + 1].overflowing_sub(src[prev_row_p + 1]).0;
            dst[dst_p + 2] = src[src_p].overflowing_sub(src[prev_row_p]).0;
            dst_p += 3;
            src_p += 3;
            prev_row_p += 3;
        }
        Ok(dst_p)
    }

    fn encode_bbp24_row2(
        dst: &mut Vec<u8>,
        mut dst_p: usize,
        src: &[u8],
        width: i16,
        y: i16,
    ) -> Result<usize> {
        let mut src_p = y as usize * width as usize * 3;
        let mut prev_row_p = (y - 1) as usize * width as usize * 3;
        dst[dst_p] = src[src_p + 2];
        dst[dst_p + 1] = src[src_p + 1];
        dst[dst_p + 2] = src[src_p];
        dst_p += 3;
        src_p += 3;
        for _ in 1..width {
            dst[dst_p] = src[src_p + 2].overflowing_sub(src[prev_row_p + 2]).0;
            dst[dst_p + 1] = src[src_p + 1].overflowing_sub(src[prev_row_p + 1]).0;
            dst[dst_p + 2] = src[src_p].overflowing_sub(src[prev_row_p]).0;
            dst_p += 3;
            src_p += 3;
            prev_row_p += 3;
        }
        Ok(dst_p)
    }

    fn encode_bbp24_row3(
        dst: &mut Vec<u8>,
        mut dst_p: usize,
        src: &[u8],
        width: i16,
        y: i16,
    ) -> Result<usize> {
        let mut src_p = y as usize * width as usize * 3;
        let mut prev_row_p = (y - 1) as usize * width as usize * 3 + 3;
        for _ in 0..width - 1 {
            dst[dst_p] = src[src_p + 2].overflowing_sub(src[prev_row_p + 2]).0;
            dst[dst_p + 1] = src[src_p + 1].overflowing_sub(src[prev_row_p + 1]).0;
            dst[dst_p + 2] = src[src_p].overflowing_sub(src[prev_row_p]).0;
            dst_p += 3;
            src_p += 3;
            prev_row_p += 3;
        }
        dst[dst_p] = src[src_p + 2];
        dst[dst_p + 1] = src[src_p + 1];
        dst[dst_p + 2] = src[src_p];
        dst_p += 3;
        Ok(dst_p)
    }

    fn encode_bbp24_row4(
        dst: &mut Vec<u8>,
        mut dst_p: usize,
        src: &[u8],
        width: i16,
        y: i16,
    ) -> Result<usize> {
        let src_p = y as usize * width as usize * 3;
        for offset in 0..3 {
            let mut src_c = src_p + 2 - offset as usize;
            let mut remaining = width;
            let value = src[src_c];
            src_c += 3;
            dst[dst_p] = value;
            dst_p += 1;
            remaining -= 1;
            if remaining == 0 {
                continue;
            }
            let mut count = 0;
            loop {
                if count as i16 >= remaining || count >= 255 || src[src_c] != value {
                    break;
                }
                src_c += 3;
                count += 1;
            }
            if count > 0 {
                dst[dst_p] = value;
                dst_p += 1;
                dst[dst_p] = count;
                dst_p += 1;
                remaining -= count as i16;
            }
            while remaining > 0 {
                let value = src[src_c];
                src_c += 3;
                dst[dst_p] = value;
                dst_p += 1;
                remaining -= 1;
                if remaining == 0 {
                    break;
                }
                let mut count = 0;
                loop {
                    if count as i16 >= remaining || count >= 255 || src[src_c] != value {
                        break;
                    }
                    src_c += 3;
                    count += 1;
                }
                if count > 0 {
                    dst[dst_p] = value;
                    dst_p += 1;
                    dst[dst_p] = count;
                    dst_p += 1;
                    remaining -= count as i16;
                }
            }
        }
        Ok(dst_p)
    }

    fn encode_image_bbp24(src: &[u8], width: i16, height: i16, row_type: &[u8]) -> Result<Vec<u8>> {
        let size = width as usize * height as usize * 3 + height as usize;
        let mut dst = Vec::with_capacity(size);
        dst.resize(size, 0);
        let mut dst_p = 0;
        for y in 0..height {
            let data = row_type[y as usize];
            dst[dst_p] = data;
            dst_p += 1;
            match data {
                0 => {
                    dst_p = Self::encode_bbp24_row0(&mut dst, dst_p, src, width, y)?;
                }
                1 => {
                    dst_p = Self::encode_bbp24_row1(&mut dst, dst_p, src, width, y)?;
                }
                2 => {
                    dst_p = Self::encode_bbp24_row2(&mut dst, dst_p, src, width, y)?;
                }
                3 => {
                    dst_p = Self::encode_bbp24_row3(&mut dst, dst_p, src, width, y)?;
                }
                4 => {
                    eprintln!("Encoding row type 4 on y={}", y);
                    dst_p = Self::encode_bbp24_row4(&mut dst, dst_p, src, width, y)?;
                }
                _ => {
                    return Err(anyhow::anyhow!("Invalid row type: {}", data));
                }
            }
        }
        Ok(dst)
    }

    fn encode_bbp32_row0(
        dst: &mut Vec<u8>,
        mut dst_p: usize,
        src: &[u8],
        width: i16,
        y: i16,
    ) -> Result<usize> {
        let mut src_p = y as usize * width as usize * 4;
        dst[dst_p] = 0xff - src[src_p + 3];
        dst[dst_p + 1] = src[src_p + 2];
        dst[dst_p + 2] = src[src_p + 1];
        dst[dst_p + 3] = src[src_p];
        dst_p += 4;
        src_p += 4;
        for _ in 1..width {
            dst[dst_p] = (0xff - src[src_p + 3])
                .overflowing_sub(0xff - src[src_p - 1])
                .0;
            dst[dst_p + 1] = src[src_p + 2].overflowing_sub(src[src_p - 2]).0;
            dst[dst_p + 2] = src[src_p + 1].overflowing_sub(src[src_p - 3]).0;
            dst[dst_p + 3] = src[src_p].overflowing_sub(src[src_p - 4]).0;
            dst_p += 4;
            src_p += 4;
        }
        return Ok(dst_p);
    }

    fn encode_bbp32_row1(
        dst: &mut Vec<u8>,
        mut dst_p: usize,
        src: &[u8],
        width: i16,
        y: i16,
    ) -> Result<usize> {
        let mut src_p = y as usize * width as usize * 4;
        let mut prev_row_p = (y - 1) as usize * width as usize * 4;
        for _ in 0..width {
            dst[dst_p] = (0xff - src[src_p + 3])
                .overflowing_sub(0xff - src[prev_row_p + 3])
                .0;
            dst[dst_p + 1] = src[src_p + 2].overflowing_sub(src[prev_row_p + 2]).0;
            dst[dst_p + 2] = src[src_p + 1].overflowing_sub(src[prev_row_p + 1]).0;
            dst[dst_p + 3] = src[src_p].overflowing_sub(src[prev_row_p]).0;
            dst_p += 4;
            src_p += 4;
            prev_row_p += 4;
        }
        Ok(dst_p)
    }

    fn encode_bbp32_row2(
        dst: &mut Vec<u8>,
        mut dst_p: usize,
        src: &[u8],
        width: i16,
        y: i16,
    ) -> Result<usize> {
        let mut src_p = y as usize * width as usize * 4;
        let mut prev_row_p = (y - 1) as usize * width as usize * 4;
        dst[dst_p] = 0xff - src[src_p + 3];
        dst[dst_p + 1] = src[src_p + 2];
        dst[dst_p + 2] = src[src_p + 1];
        dst[dst_p + 3] = src[src_p];
        dst_p += 4;
        src_p += 4;
        for _ in 1..width {
            dst[dst_p] = (0xff - src[src_p + 3])
                .overflowing_sub(0xff - src[prev_row_p + 3])
                .0;
            dst[dst_p + 1] = src[src_p + 2].overflowing_sub(src[prev_row_p + 2]).0;
            dst[dst_p + 2] = src[src_p + 1].overflowing_sub(src[prev_row_p + 1]).0;
            dst[dst_p + 3] = src[src_p].overflowing_sub(src[prev_row_p]).0;
            dst_p += 4;
            src_p += 4;
            prev_row_p += 4;
        }
        Ok(dst_p)
    }

    fn encode_bbp32_row3(
        dst: &mut Vec<u8>,
        mut dst_p: usize,
        src: &[u8],
        width: i16,
        y: i16,
    ) -> Result<usize> {
        let mut src_p = y as usize * width as usize * 4;
        let mut prev_row_p = (y - 1) as usize * width as usize * 4 + 4;
        for _ in 0..width - 1 {
            dst[dst_p] = (0xff - src[src_p + 3])
                .overflowing_sub(0xff - src[prev_row_p + 3])
                .0;
            dst[dst_p + 1] = src[src_p + 2].overflowing_sub(src[prev_row_p + 2]).0;
            dst[dst_p + 2] = src[src_p + 1].overflowing_sub(src[prev_row_p + 1]).0;
            dst[dst_p + 3] = src[src_p].overflowing_sub(src[prev_row_p]).0;
            dst_p += 4;
            src_p += 4;
            prev_row_p += 4;
        }
        dst[dst_p] = 0xff - src[src_p + 3];
        dst[dst_p + 1] = src[src_p + 2];
        dst[dst_p + 2] = src[src_p + 1];
        dst[dst_p + 3] = src[src_p];
        dst_p += 4;
        Ok(dst_p)
    }

    fn encode_bbp32_row4(
        dst: &mut Vec<u8>,
        mut dst_p: usize,
        src: &[u8],
        width: i16,
        y: i16,
    ) -> Result<usize> {
        let src_p = y as usize * width as usize * 4;
        for offset in 0..4 {
            let mut src_c = src_p + 3 - offset as usize;
            let mut remaining = width;
            let value = src[src_c];
            src_c += 4;
            dst[dst_p] = if offset == 0 { 0xff - value } else { value };
            dst_p += 1;
            remaining -= 1;
            if remaining == 0 {
                continue;
            }
            let mut count = 0u8;
            loop {
                if count as i16 >= remaining || count >= 255 || src[src_c] != value {
                    break;
                }
                src_c += 4;
                count += 1;
            }
            if count > 0 {
                dst[dst_p] = if offset == 0 { 0xff - value } else { value };
                dst_p += 1;
                dst[dst_p] = count;
                dst_p += 1;
                remaining -= count as i16;
            }
            while remaining > 0 {
                let value = src[src_c];
                src_c += 4;
                dst[dst_p] = if offset == 0 { 0xff - value } else { value };
                dst_p += 1;
                remaining -= 1;
                if remaining == 0 {
                    break;
                }
                let mut count = 0u8;
                loop {
                    if count as i16 >= remaining || count >= 255 || src[src_c] != value {
                        break;
                    }
                    src_c += 4;
                    count += 1;
                }
                if count > 0 {
                    dst[dst_p] = if offset == 0 { 0xff - value } else { value };
                    dst_p += 1;
                    dst[dst_p] = count;
                    dst_p += 1;
                    remaining -= count as i16;
                }
            }
        }
        Ok(dst_p)
    }

    fn encode_image_bbp32(src: &[u8], width: i16, height: i16, row_type: &[u8]) -> Result<Vec<u8>> {
        let size = width as usize * height as usize * 4 + height as usize;
        let mut dst = Vec::with_capacity(size);
        dst.resize(size, 0);
        let mut dst_p = 0;
        for y in 0..height {
            let data = row_type[y as usize];
            dst[dst_p] = data;
            dst_p += 1;
            match data {
                0 => {
                    dst_p = Self::encode_bbp32_row0(&mut dst, dst_p, src, width, y)?;
                }
                1 => {
                    dst_p = Self::encode_bbp32_row1(&mut dst, dst_p, src, width, y)?;
                }
                2 => {
                    dst_p = Self::encode_bbp32_row2(&mut dst, dst_p, src, width, y)?;
                }
                3 => {
                    dst_p = Self::encode_bbp32_row3(&mut dst, dst_p, src, width, y)?;
                }
                4 => {
                    dst_p = Self::encode_bbp32_row4(&mut dst, dst_p, src, width, y)?;
                }
                _ => {
                    return Err(anyhow::anyhow!("Invalid row type: {} on line {}", data, y));
                }
            }
        }
        Ok(dst)
    }

    fn rgba_to_rgb(src: &[u8]) -> Vec<u8> {
        let mut dst = Vec::with_capacity(src.len() / 4 * 3);
        for chunk in src.chunks(4) {
            if chunk.len() == 4 {
                dst.push(chunk[0]); // R
                dst.push(chunk[1]); // G
                dst.push(chunk[2]); // B
            }
        }
        dst
    }

    fn rgb_to_rgba(src: &[u8]) -> Vec<u8> {
        let mut dst = Vec::with_capacity(src.len() / 3 * 4);
        for chunk in src.chunks(3) {
            if chunk.len() == 3 {
                dst.push(chunk[0]); // R
                dst.push(chunk[1]); // G
                dst.push(chunk[2]); // B
                dst.push(0xff); // A
            }
        }
        dst
    }
}
