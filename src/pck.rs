use crate::ext::ExtReader;
use anyhow::Result;
use std::io::{Read, Seek};
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
}

impl PckReader<std::io::BufReader<std::fs::File>> {
    pub fn new_from_file<P: AsRef<Path> + ?Sized>(p: &P) -> Result<Self> {
        let file = std::fs::File::open(p)?;
        let reader = std::io::BufReader::new(file);
        Self::new(reader)
    }
}
