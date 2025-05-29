use crate::ext::{ExtReader, ExtWriter};
use anyhow::Result;
use std::io::{Read, Seek, Write};
use std::iter::Iterator;
use std::path::Path;

#[derive(Debug)]
pub struct PckFileHeader {
    pub name: String,
    pub offset: u32,
    pub size: u32,
}

#[derive(Debug)]
pub struct PckFileReader<'a> {
    pub header: &'a PckFileHeader,
}

#[derive(Debug)]
pub struct PckFileReaderMut<'a, R: Read + Seek> {
    pub header: &'a PckFileHeader,
    reader: &'a mut R,
    pos: u32,
}

impl<'a, R: Read + Seek> Read for PckFileReaderMut<'a, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let bytes_to_read = buf.len().min((self.header.size - self.pos) as usize);
        if bytes_to_read == 0 {
            return Ok(0);
        }
        self.reader.seek(std::io::SeekFrom::Start(
            self.header.offset as u64 + self.pos as u64,
        ))?;
        let bytes_read = self.reader.read(&mut buf[..bytes_to_read])?;
        self.pos += bytes_read as u32;
        Ok(bytes_read)
    }
}

impl<'a, R: Read + Seek> Seek for PckFileReaderMut<'a, R> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        let new_pos = match pos {
            std::io::SeekFrom::Start(offset) => offset,
            std::io::SeekFrom::End(offset) => (self.header.size as i64 + offset) as u64,
            std::io::SeekFrom::Current(offset) => (self.pos as i64 + offset) as u64,
        };
        if new_pos > self.header.size as u64 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Seek position out of bounds",
            ));
        }
        self.pos = new_pos as u32;
        Ok(new_pos)
    }

    fn stream_position(&mut self) -> std::io::Result<u64> {
        Ok(self.pos as u64)
    }

    fn rewind(&mut self) -> std::io::Result<()> {
        self.pos = 0;
        Ok(())
    }
}

pub struct PckFileReaderIter<'a, T: Iterator<Item = &'a PckFileHeader>> {
    header_iter: T,
}

impl<'a, T: Iterator<Item = &'a PckFileHeader>> Iterator for PckFileReaderIter<'a, T> {
    type Item = PckFileReader<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(header) = self.header_iter.next() {
            Some(PckFileReader { header })
        } else {
            None
        }
    }
}

pub struct PckFileReaderMutIter<'a, R: Read + Seek, T: Iterator<Item = &'a PckFileHeader>> {
    header_iter: T,
    reader: &'a mut R,
}

impl<'a, R: Read + Seek, T: Iterator<Item = &'a PckFileHeader>> Iterator
    for PckFileReaderMutIter<'a, R, T>
{
    type Item = PckFileReaderMut<'a, R>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(header) = self.header_iter.next() {
            // SAFETY: We know that self.reader lives for the entire 'a lifetime
            // and we're only returning one mutable reference at a time through the iterator
            let reader_ref = unsafe { std::mem::transmute::<&mut R, &'a mut R>(self.reader) };
            Some(PckFileReaderMut {
                header,
                reader: reader_ref,
                pos: 0,
            })
        } else {
            None
        }
    }
}

pub struct PckReader<T: Read + Seek> {
    reader: T,
    file_headers: Vec<PckFileHeader>,
}

impl<T: Read + Seek> PckReader<T> {
    pub fn new(mut reader: T) -> Result<Self> {
        let count = reader.read_u32()?;
        // (offset, size)
        let mut offset_list = Vec::new();
        for _ in 0..count {
            let offset = reader.read_u32()?;
            let size = reader.read_u32()?;
            offset_list.push((offset, size));
        }
        let mut file_headers = Vec::new();
        for i in 0..count {
            let name = reader.read_cstring_with_size(0x38)?;
            let offset = reader.read_u32()?;
            let size = reader.read_u32()?;
            let ori_offset = offset_list[i as usize];
            if ori_offset.0 != offset || ori_offset.1 != size {
                return Err(anyhow::anyhow!(
                    "Offset or size mismatch for file {}: expected ({}, {}), got ({}, {})",
                    name,
                    ori_offset.0,
                    ori_offset.1,
                    offset,
                    size
                ));
            }
            file_headers.push(PckFileHeader { name, offset, size });
        }
        Ok(PckReader {
            reader,
            file_headers,
        })
    }

    pub fn iter<'a>(&'a self) -> PckFileReaderIter<'a, impl Iterator<Item = &'a PckFileHeader>> {
        return PckFileReaderIter {
            header_iter: self.file_headers.iter(),
        };
    }

    pub fn iter_mut<'a>(
        &'a mut self,
    ) -> PckFileReaderMutIter<'a, T, impl Iterator<Item = &'a PckFileHeader>> {
        return PckFileReaderMutIter {
            header_iter: self.file_headers.iter(),
            reader: &mut self.reader,
        };
    }

    pub fn len(&self) -> usize {
        self.file_headers.len()
    }
}

impl PckReader<std::io::BufReader<std::fs::File>> {
    pub fn new_from_file<P: AsRef<Path> + ?Sized>(p: &P) -> Result<Self> {
        let file = std::fs::File::open(p)?;
        let reader = std::io::BufReader::new(file);
        Self::new(reader)
    }
}

pub struct PckFileWriter<'a, T: Write + Seek> {
    header: &'a mut PckFileHeader,
    writer: &'a mut T,
}

impl<'a, T: Write + Seek> Write for PckFileWriter<'a, T> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.writer.seek(std::io::SeekFrom::Start(
            self.header.offset as u64 + self.header.size as u64,
        ))?;
        let bytes_written = self.writer.write(buf)?;
        self.header.size += bytes_written as u32;
        Ok(bytes_written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

pub struct PckWriter<T: Write + Seek + Read> {
    file: T,
    file_headers: Vec<PckFileHeader>,
    header_max_size: u32,
}

impl<T: Write + Seek + Read> PckWriter<T> {
    pub fn new(file: T, header_max_size: u32) -> Self {
        PckWriter {
            file,
            file_headers: Vec::new(),
            header_max_size,
        }
    }

    pub fn add_file<'a, S: AsRef<str> + ?Sized>(
        &'a mut self,
        name: &S,
    ) -> Result<PckFileWriter<'a, T>> {
        let offset = self
            .file_headers
            .last()
            .map(|h| h.offset + h.size)
            .unwrap_or(self.header_max_size);
        self.file_headers.push(PckFileHeader {
            name: name.as_ref().to_owned(),
            offset,
            size: 0,
        });
        self.check_header_capacity()?;
        let header = self.file_headers.last_mut().unwrap();
        Ok(PckFileWriter {
            header,
            writer: &mut self.file,
        })
    }

    pub fn write_header(&mut self) -> Result<()> {
        self.file.seek(std::io::SeekFrom::Start(0))?;
        self.file.write_u32(self.file_headers.len() as u32)?;
        for header in &self.file_headers {
            self.file.write_u32(header.offset)?;
            self.file.write_u32(header.size)?;
        }
        for header in &self.file_headers {
            self.file.write_cstring_with_size(&header.name, 0x38)?;
            self.file.write_u32(header.offset)?;
            self.file.write_u32(header.size)?;
        }
        self.file.flush()?;
        Ok(())
    }

    fn check_header_capacity(&mut self) -> Result<()> {
        if self.file_headers.len() as u32 * 0x48 + 4 < self.header_max_size {
            return Ok(());
        }
        let new_header_capacity = self.header_max_size + 0x800;
        self.file
            .seek(std::io::SeekFrom::Start(self.header_max_size as u64))?;
        let mut buffer1 = [0; 0x800];
        let mut buffer2 = [0; 0x800];
        let mut buffer1_size = 0x800;
        let mut buffer1_start = self.header_max_size as u64;
        let mut buffer2_size;
        loop {
            buffer2_size = self.file.read(&mut buffer2)?;
            self.file.seek(std::io::SeekFrom::Start(buffer1_start))?;
            self.file.write_all(&buffer1[..buffer1_size])?;
            buffer1_size = buffer2_size;
            std::mem::swap(&mut buffer1, &mut buffer2);
            buffer1_start += buffer1_size as u64;
            if buffer2_size == 0 {
                break;
            }
        }
        for header in &mut self.file_headers {
            header.offset += 0x800;
        }
        self.header_max_size = new_header_capacity;
        self.file.flush()?;
        Ok(())
    }
}

impl PckWriter<std::fs::File> {
    pub fn calculate_header_size(file_count: u32) -> u32 {
        let mut header_size = file_count * 0x48 + 4;
        let a = header_size % 0x800;
        if a != 0 {
            header_size += 0x800 - a;
        }
        header_size
    }

    pub fn new_from_file<P: AsRef<Path> + ?Sized>(p: &P, header_max_size: u32) -> Result<Self> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .create(true)
            .write(true)
            .truncate(true)
            .open(p)?;
        Ok(Self::new(file, header_max_size))
    }
}
