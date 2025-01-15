#[derive(Debug, Default, Clone, Copy)]
pub enum DecompressionByteLimit {
    #[default]
    FourMegaBytes,
    OneGigaByte,
}
