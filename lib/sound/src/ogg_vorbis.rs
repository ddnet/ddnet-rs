use std::io::Cursor;

use anyhow::anyhow;
use symphonia::{
    core::{
        codecs::{Decoder, DecoderOptions},
        formats::FormatReader,
        io::MediaSourceStream,
    },
    default::{codecs::VorbisDecoder, formats::OggReader},
};

pub fn verify_ogg_vorbis(file: &[u8]) -> anyhow::Result<()> {
    let mut reader = OggReader::try_new(
        MediaSourceStream::new(Box::new(Cursor::new(file.to_vec())), Default::default()),
        &Default::default(),
    )?;
    anyhow::ensure!(
        reader.tracks().len() == 1,
        "currently exactly one track in an ogg vorbis file is allowed."
    );
    let mut decoder = VorbisDecoder::try_new(
        &reader
            .default_track()
            .ok_or_else(|| anyhow!("no default track found"))?
            .codec_params,
        &DecoderOptions { verify: true },
    )?;
    loop {
        match reader.next_packet() {
            Ok(packet) => {
                let _ = decoder.decode(&packet)?;
            }
            Err(err) => match err {
                symphonia::core::errors::Error::IoError(error) => {
                    if matches!(error.kind(), std::io::ErrorKind::UnexpectedEof) {
                        break;
                    } else {
                        return Err(error.into());
                    }
                }
                _ => return Err(err.into()),
            },
        }
    }
    if decoder.finalize().verify_ok.unwrap_or(true) {
        Ok(())
    } else {
        Err(anyhow!("Vorbis verify check failed."))
    }
}
