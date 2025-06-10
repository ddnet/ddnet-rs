use std::{
    io::{Read, Write},
    path::Path,
};

use serde::{de::DeserializeOwned, Serialize};

/// Decompresses a compressed file into an uncompressed file. Returns the bytes read
/// ### Prefer this method over using compression algorithms yourself, because it has side effects
pub fn decompress(file: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut uncompressed_file: Vec<u8> = Default::default();
    #[cfg(not(feature = "rust_zstd"))]
    {
        let mut dec = zstd::Decoder::new(file)?;
        dec.read_to_end(&mut uncompressed_file)?;
        dec.finish();
    }
    #[cfg(feature = "rust_zstd")]
    {
        let mut decoder = ruzstd::decoding::StreamingDecoder::new(file)?;
        decoder.read_to_end(&mut uncompressed_file)?;
    }
    Ok(uncompressed_file)
}

/// Compresses an uncompressed file into a compressed file.
/// ### Prefer this method over using compression algorithms yourself. It has side effects
pub fn compress(uncompressed_file: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut write_data: Vec<u8> = Default::default();
    // Compression level 15 seems to be a good trait performance vs map size
    // Tested with the test benchmark in this crate on some maps.
    let mut enc = zstd::Encoder::new(&mut write_data, 15)?;
    enc.write_all(uncompressed_file)?;
    enc.finish()?;
    Ok(write_data)
}

const TWMAP_BINCODE: &str = "twmap_bincode";

/// Deserializes the given type from the internal twmap bincode format.
pub fn deserialize_twmap_bincode<T: DeserializeOwned>(
    uncompressed_file: &[u8],
) -> anyhow::Result<T> {
    let (ty, read_size) = bincode::serde::decode_from_slice::<String, _>(
        uncompressed_file,
        bincode::config::standard(),
    )?;
    let uncompressed_file = &uncompressed_file[read_size..];
    let (expected_size, read_size) = bincode::serde::decode_from_slice::<u64, _>(
        uncompressed_file,
        bincode::config::standard(),
    )?;
    let uncompressed_file = &uncompressed_file[read_size..];
    anyhow::ensure!(
        ty == TWMAP_BINCODE,
        "given file is not of type {TWMAP_BINCODE}"
    );
    let (res, read_size) =
        bincode::serde::decode_from_slice::<T, _>(uncompressed_file, bincode::config::standard())?;
    anyhow::ensure!(
        read_size as u64 == expected_size,
        "deserialization size is wrong, expected {expected_size} got {read_size}"
    );
    Ok(res)
}

/// Serializes the given type to the internal twmap bincode format.
/// Returns the amount of bytes written.
pub fn serialize_twmap_bincode<T: Serialize, W: std::io::Write>(
    value: &T,
    writer: &mut W,
) -> anyhow::Result<usize> {
    let written_bytes_str =
        bincode::serde::encode_into_std_write(TWMAP_BINCODE, writer, bincode::config::standard())?;
    let mut tmp_writer: Vec<u8> = Default::default();
    let written_bytes =
        bincode::serde::encode_into_std_write(value, &mut tmp_writer, bincode::config::standard())?;
    let written_bytes_file_size = bincode::serde::encode_into_std_write(
        tmp_writer.len() as u64,
        writer,
        bincode::config::standard(),
    )?;
    writer.write_all(&tmp_writer)?;
    Ok(written_bytes_str + written_bytes_file_size + written_bytes)
}

/// Serializes the given type to the internal twmap bincode format.
/// Returns the amount of bytes written.
pub fn verify_twmap_bincode(file: &[u8]) -> anyhow::Result<()> {
    let (ty, read_size) =
        bincode::serde::decode_from_slice::<String, _>(file, bincode::config::standard())?;
    let file = &file[read_size..];
    let (expected_size, read_size) =
        bincode::serde::decode_from_slice::<u64, _>(file, bincode::config::standard())?;
    let file = &file[read_size..];
    anyhow::ensure!(
        ty == TWMAP_BINCODE,
        "given file is not of type {TWMAP_BINCODE}"
    );
    anyhow::ensure!(
        file.len() as u64 == expected_size,
        "deserialization size is wrong, expected {expected_size} got {read_size}"
    );
    Ok(())
}

/// Get's the file extension of the given path or if the file name ends with
/// `.twmap.tar` it returns `twmap.tar`.
pub fn file_ext_or_twmap_tar(path: &Path) -> Option<&str> {
    let ext = path.extension().and_then(|e| e.to_str())?;
    let stem = path.file_stem().and_then(|e| e.to_str())?;
    Some(if stem.ends_with("twmap") && ext == "tar" {
        "twmap.tar"
    } else {
        ext
    })
}
