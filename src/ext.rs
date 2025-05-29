use std::io::{Read, Result, Write};

pub trait ExtWriter {
    fn write_i16(&mut self, value: i16) -> Result<()>;
    fn write_i32(&mut self, value: i32) -> Result<()>;
}

impl<W: Write> ExtWriter for W {
    fn write_i16(&mut self, value: i16) -> Result<()> {
        let bytes = value.to_le_bytes();
        self.write_all(&bytes)
    }

    fn write_i32(&mut self, value: i32) -> Result<()> {
        let bytes = value.to_le_bytes();
        self.write_all(&bytes)
    }
}

pub trait ExtReader {
    fn read_i16(&mut self) -> Result<i16>;
    fn read_i32(&mut self) -> Result<i32>;
    fn read_u32(&mut self) -> Result<u32>;
    fn read_cstring_with_size(&mut self, size: usize) -> Result<String>;
}

impl<R: Read> ExtReader for R {
    fn read_i16(&mut self) -> Result<i16> {
        let mut buffer = [0; 2];
        self.read_exact(&mut buffer)?;
        Ok(i16::from_le_bytes(buffer))
    }

    fn read_i32(&mut self) -> Result<i32> {
        let mut buffer = [0; 4];
        self.read_exact(&mut buffer)?;
        Ok(i32::from_le_bytes(buffer))
    }

    fn read_u32(&mut self) -> Result<u32> {
        let mut buffer = [0; 4];
        self.read_exact(&mut buffer)?;
        Ok(u32::from_le_bytes(buffer))
    }

    fn read_cstring_with_size(&mut self, size: usize) -> Result<String> {
        let mut buffer = vec![0; size];
        self.read_exact(&mut buffer)?;
        let null_pos = buffer.iter().position(|&c| c == 0).unwrap_or(size);
        let s = str::from_utf8(&buffer[..null_pos])
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(s.to_owned())
    }
}
