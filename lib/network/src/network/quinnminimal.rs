//! Commonly used code in most examples.

use anyhow::anyhow;
use base::hash::Hash;
use ed25519_dalek::pkcs8::EncodePrivateKey;
use quinn::{
    AckFrequencyConfig, ClientConfig, Endpoint, IdleTimeout, ServerConfig, TransportConfig, VarInt,
    crypto::rustls::{QuicClientConfig, QuicServerConfig},
};
use rustls::{crypto::CryptoProvider, pki_types::PrivateKeyDer};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use x509_cert::der::{Decode, Encode};

use super::{
    traits::NUM_BIDI_STREAMS,
    types::{
        NetworkClientCertCheckMode, NetworkClientCertMode, NetworkClientInitOptions,
        NetworkServerCertAndKey, NetworkServerCertMode, NetworkServerCertModeResult,
        NetworkServerInitOptions,
    },
};

/// Constructs a QUIC endpoint configured for use a client only.
///
/// ## Args
///
/// - server_certs: list of trusted certificates.
pub fn make_client_endpoint(
    bind_addr: SocketAddr,
    options: &NetworkClientInitOptions,
) -> anyhow::Result<Endpoint> {
    let client_cfg = configure_client(options)?;
    let mut endpoint = Endpoint::client(bind_addr)?;
    endpoint.set_default_client_config(client_cfg);
    Ok(endpoint)
}

/// Constructs a QUIC endpoint configured to listen for incoming connections on a certain address
/// and port.
///
/// ## Returns
///
/// - a stream of incoming QUIC connections
/// - server certificate serialized into DER format
pub fn make_server_endpoint(
    bind_addr: SocketAddr,
    cert_mode: NetworkServerCertMode,
    options: &NetworkServerInitOptions,
) -> anyhow::Result<(Endpoint, NetworkServerCertModeResult)> {
    let (server_config, server_cert) = configure_server(cert_mode, options)?;
    //eprintln!("{:#?}", server_config);
    let endpoint = Endpoint::server(server_config, bind_addr)?;
    Ok((endpoint, server_cert))
}

/// dummy certificate handler
#[derive(Debug)]
struct SkipServerVerification(rustls::crypto::CryptoProvider);

impl SkipServerVerification {
    fn new() -> Arc<Self> {
        Arc::new(Self(rustls::crypto::ring::default_provider()))
    }
}

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// hash certificate handler
#[derive(Debug)]
struct CertHashServerVerification {
    hash: Hash,
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl CertHashServerVerification {
    fn new(provider: Arc<rustls::crypto::CryptoProvider>, hash: Hash) -> Arc<Self> {
        Arc::new(Self { hash, provider })
    }
}

impl rustls::client::danger::ServerCertVerifier for CertHashServerVerification {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let cert = x509_cert::Certificate::from_der(end_entity).map_err(|_| {
            rustls::Error::InvalidCertificate(rustls::CertificateError::BadEncoding)
        })?;

        let hash = cert
            .tbs_certificate
            .subject_public_key_info
            .fingerprint_bytes()
            .map_err(|_| {
                rustls::Error::InvalidCertificate(rustls::CertificateError::BadSignature)
            })?;
        if self.hash.eq(&hash) {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::BadSignature,
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Builds default quinn client config and trusts given certificates.
///
/// ## Args
///
/// - server_certs: a list of trusted certificates in DER format.
fn configure_client(options: &NetworkClientInitOptions) -> anyhow::Result<ClientConfig> {
    let mut transport_config = TransportConfig::default();
    transport_config
        .max_concurrent_bidi_streams(NUM_BIDI_STREAMS.into())
        .max_concurrent_uni_streams(50u32.into());
    if let Some(stream_receive_window) = options.base.stream_receive_window {
        transport_config.stream_receive_window(stream_receive_window.into());
    }
    if let Some(receive_window) = options.base.receive_window {
        transport_config.receive_window(receive_window.into());
    }
    if let Some(send_window) = options.base.send_window {
        transport_config.send_window(send_window.into());
    }
    if options
        .base
        .timeout
        .is_some_and(|timeout| timeout != Duration::ZERO)
    {
        transport_config
            .max_idle_timeout(IdleTimeout::try_from(options.base.timeout.unwrap()).ok());
    }
    if let Some(max_reorder) = options.base.packet_reorder_threshold {
        transport_config.packet_threshold(max_reorder.clamp(3, u32::MAX));
    }
    if let Some(max_reorder) = options.base.packet_time_threshold {
        transport_config.time_threshold(max_reorder);
    }

    if let Some(((ack_eliciting_threshold, max_ack_delay), ack_reordering_threshold)) = options
        .base
        .ack_eliciting_threshold
        .zip(options.base.max_ack_delay)
        .zip(options.base.ack_reordering_threshold)
    {
        let mut freq = AckFrequencyConfig::default();
        freq.ack_eliciting_threshold(VarInt::from_u32(ack_eliciting_threshold));
        freq.max_ack_delay(Some(max_ack_delay));
        freq.reordering_threshold(VarInt::from_u32(ack_reordering_threshold));
        transport_config.ack_frequency_config(Some(freq));
    }
    let transport = Arc::new(transport_config);

    let (cert, priv_key) = match &options.cert {
        NetworkClientCertMode::FromCertAndPrivateKey { cert, private_key } => (
            vec![cert.to_der()?.into()],
            PrivateKeyDer::try_from(private_key.to_pkcs8_der()?.as_bytes().to_vec())
                .map_err(|_| anyhow!("could not convert private key to der"))?,
        ),
    };

    match &options.cert_check {
        NetworkClientCertCheckMode::CheckByCert { cert: server_cert } => {
            let mut certs = rustls::RootCertStore::empty();
            certs.add(server_cert.to_vec().into())?;

            if CryptoProvider::get_default().is_none() {
                CryptoProvider::install_default(rustls::crypto::ring::default_provider())
                    .map_err(|_| anyhow!("ring crypto provider could not be initialized"))?;
            }
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            let mut client_config = ClientConfig::new(Arc::new(QuicClientConfig::try_from({
                rustls::ClientConfig::builder_with_provider(provider)
                    .with_safe_default_protocol_versions()?
                    .with_root_certificates(certs)
                    .with_client_auth_cert(cert, priv_key)?
            })?));
            client_config.transport_config(transport);
            Ok(client_config)
        }
        NetworkClientCertCheckMode::CheckByPubKeyHash { hash } => {
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            let mut client_config = ClientConfig::new(Arc::new(QuicClientConfig::try_from({
                rustls::ClientConfig::builder_with_provider(provider.clone())
                    .with_safe_default_protocol_versions()?
                    .dangerous()
                    .with_custom_certificate_verifier(CertHashServerVerification::new(
                        provider, **hash,
                    ))
                    .with_client_auth_cert(cert, priv_key)?
            })?));
            client_config.transport_config(transport);
            Ok(client_config)
        }
        NetworkClientCertCheckMode::DisableCheck => {
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            let mut client_config = ClientConfig::new(Arc::new(QuicClientConfig::try_from({
                rustls::ClientConfig::builder_with_provider(provider)
                    .with_safe_default_protocol_versions()?
                    .dangerous()
                    .with_custom_certificate_verifier(SkipServerVerification::new())
                    .with_client_auth_cert(cert, priv_key)?
            })?));
            client_config.transport_config(transport);
            Ok(client_config)
        }
    }
}

#[derive(Debug)]
struct ServerClientCertVerifier(Arc<CryptoProvider>);

impl rustls::server::danger::ClientCertVerifier for ServerClientCertVerifier {
    fn root_hint_subjects(&self) -> &[rustls::DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::server::danger::ClientCertVerified, rustls::Error> {
        // just make sure that the cert is x509
        // the client cert is used for identification on the server.
        x509_cert::Certificate::from_der(end_entity)
            .map_err(|_| rustls::Error::InvalidCertificate(rustls::CertificateError::BadEncoding))
            .map(|_| rustls::server::danger::ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// Returns default server configuration along with its certificate.
fn configure_server(
    cert_mode: NetworkServerCertMode,
    options: &NetworkServerInitOptions,
) -> anyhow::Result<(ServerConfig, NetworkServerCertModeResult)> {
    let (priv_key, cert_chain, cert) = match cert_mode {
        NetworkServerCertMode::FromCertAndPrivateKey(cert_and_key) => {
            let NetworkServerCertAndKey { cert, private_key } = *cert_and_key;
            let priv_key = private_key.to_pkcs8_der()?;
            (
                PrivateKeyDer::try_from(priv_key.as_bytes().to_vec())
                    .map_err(|_| anyhow!("converting cert failed"))?,
                vec![cert.to_der()?.to_vec().into()],
                NetworkServerCertModeResult::Cert {
                    cert: Box::new(cert),
                },
            )
        }
    };

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut server_config = ServerConfig::with_crypto(Arc::new(QuicServerConfig::try_from(
        rustls::ServerConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()?
            .with_client_cert_verifier(Arc::new(ServerClientCertVerifier(provider.clone())))
            .with_single_cert(cert_chain, priv_key)?,
    )?));
    // For now disable migration. Before enabling it think about ip bans
    // since those would be affected most by this. Maybe account-only
    // servers could allow it or similar.
    server_config.migration(false);
    let transport = Arc::get_mut(&mut server_config.transport).unwrap();

    // IMPORTANT: the number of bidi streams must be exactly like the values used in the implementations
    // Look for variables called NUM_BIDI_STREAMS
    transport
        .max_concurrent_bidi_streams(NUM_BIDI_STREAMS.into())
        .max_concurrent_uni_streams(20u32.into());

    if let Some(stream_receive_window) = options.base.stream_receive_window {
        transport.stream_receive_window(stream_receive_window.into());
    } else {
        transport.stream_receive_window((1024u32 * 64u32).into());
    }
    if let Some(receive_window) = options.base.receive_window {
        transport.receive_window(receive_window.into());
    }
    if let Some(send_window) = options.base.send_window {
        transport.send_window(send_window.into());
    }
    if let Some(max_reorder) = options.base.packet_reorder_threshold {
        transport.packet_threshold(max_reorder.clamp(3, u32::MAX));
    }
    if let Some(max_reorder) = options.base.packet_time_threshold {
        transport.time_threshold(max_reorder);
    }

    if let Some(((ack_eliciting_threshold, max_ack_delay), ack_reordering_threshold)) = options
        .base
        .ack_eliciting_threshold
        .zip(options.base.max_ack_delay)
        .zip(options.base.ack_reordering_threshold)
    {
        let mut freq = AckFrequencyConfig::default();
        freq.ack_eliciting_threshold(VarInt::from_u32(ack_eliciting_threshold));
        freq.max_ack_delay(Some(max_ack_delay));
        freq.reordering_threshold(VarInt::from_u32(ack_reordering_threshold));
        transport.ack_frequency_config(Some(freq));
    }

    if options
        .base
        .timeout
        .is_some_and(|timeout| timeout != Duration::ZERO)
    {
        transport.max_idle_timeout(IdleTimeout::try_from(options.base.timeout.unwrap()).ok());
    }

    transport.keep_alive_interval(options.keep_alive.or(Some(Duration::from_millis(1000))));

    Ok((server_config, cert))
}
