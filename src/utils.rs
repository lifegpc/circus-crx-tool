use std::io::Write;
use std::path::Path;
use zstd::Encoder;

pub fn make_sure_dir_exists<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
    let path = match path.as_ref().parent() {
        Some(parent) => parent,
        None => return Ok(()),
    };
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}

pub fn compress_data(data: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut encoder = Encoder::new(Vec::new(), 32)?;
    encoder.write_all(data)?;
    Ok(encoder.finish()?)
}

pub fn decompress_data(data: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut decoder = zstd::Decoder::new(data)?;
    let mut decompressed_data = Vec::new();
    std::io::copy(&mut decoder, &mut decompressed_data)?;
    Ok(decompressed_data)
}
