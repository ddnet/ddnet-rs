use base::hash::Hash;
use hiarc::Hiarc;

#[derive(Debug, Clone, Hiarc)]
pub enum ServerCertMode {
    Cert(Vec<u8>),
    Hash(Hash),
    /// The game will try to get the mode automatically
    Unknown,
}
