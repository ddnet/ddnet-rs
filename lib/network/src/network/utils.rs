use ed25519_dalek::{SigningKey, pkcs8::EncodePrivateKey};
use rcgen::{CertificateParams, KeyPair, PKCS_ED25519};
use spki::der::{Decode, pem::LineEnding};

pub fn create_certifified_keys() -> (x509_cert::Certificate, SigningKey) {
    let mut rng = rand::rngs::OsRng;
    let private_key = SigningKey::generate(&mut rng);

    let key = private_key.to_pkcs8_pem(LineEnding::LF).unwrap();
    let key_pair = KeyPair::from_pkcs8_pem_and_sign_algo(&key, &PKCS_ED25519).unwrap();
    let cert = CertificateParams::new(vec!["localhost".into()])
        .unwrap()
        .self_signed(&key_pair)
        .unwrap();

    // yep, this is stupid, didn't get x509_cert to work with ed25519 keys
    (
        x509_cert::Certificate::from_der(cert.der()).unwrap(),
        private_key,
    )
}
