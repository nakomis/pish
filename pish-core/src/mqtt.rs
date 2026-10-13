//! MQTT publishing for widgets.
//!
//! Widgets talk to the broker only through the [`MqttPublisher`] trait, which
//! lives on [`crate::Services`]. Production uses [`RumqttcPublisher`] (AWS IoT
//! Core over mutual TLS); tests substitute a mock.

use std::sync::Arc;
use std::time::Duration;

use rumqttc::{Client, MqttOptions, QoS, TlsConfiguration, Transport};

use crate::config::MqttConfig;

#[derive(Debug)]
pub enum MqttError {
    /// Reading or parsing the device certificate / key failed.
    Tls(String),
    /// The message could not be queued for sending.
    Publish(String),
}

impl std::fmt::Display for MqttError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MqttError::Tls(m) => write!(f, "MQTT TLS setup: {m}"),
            MqttError::Publish(m) => write!(f, "MQTT publish: {m}"),
        }
    }
}

impl std::error::Error for MqttError {}

/// Something that can publish to an MQTT broker.
///
/// Implementations must not block: `publish` is called from the UI thread.
pub trait MqttPublisher: Send + Sync {
    /// Queue `payload` for delivery on `topic` (QoS 1, not retained).
    fn publish(&self, topic: &str, payload: &[u8]) -> Result<(), MqttError>;

    /// The MQTT client id this publisher connects as (the IoT thing name).
    fn client_id(&self) -> &str;
}

/// Production publisher backed by rumqttc's synchronous client. The network
/// event loop runs on a background thread and reconnects on its own.
pub struct RumqttcPublisher {
    client: Client,
    client_id: String,
}

impl RumqttcPublisher {
    /// Build the TLS client config and start the connection thread. Returns an
    /// error only for local problems (bad cert/key); broker connectivity
    /// problems are logged and retried in the background.
    pub fn connect(cfg: &MqttConfig) -> Result<Self, MqttError> {
        let tls = tls_config(cfg)?;

        let mut opts = MqttOptions::new(&cfg.client_id, &cfg.endpoint, cfg.port);
        opts.set_keep_alive(Duration::from_secs(30));
        opts.set_transport(Transport::tls_with_config(TlsConfiguration::Rustls(Arc::new(tls))));

        let (client, mut connection) = Client::new(opts, 16);
        std::thread::Builder::new()
            .name("pish-mqtt".into())
            .spawn(move || {
                for event in connection.iter() {
                    if let Err(e) = event {
                        log::warn!("MQTT connection error: {e}");
                        std::thread::sleep(Duration::from_secs(5));
                    }
                }
            })
            .map_err(|e| MqttError::Publish(format!("spawning MQTT thread: {e}")))?;

        Ok(Self { client, client_id: cfg.client_id.clone() })
    }
}

impl MqttPublisher for RumqttcPublisher {
    fn publish(&self, topic: &str, payload: &[u8]) -> Result<(), MqttError> {
        self.client
            .try_publish(topic, QoS::AtLeastOnce, false, payload.to_vec())
            .map_err(|e| MqttError::Publish(e.to_string()))
    }

    fn client_id(&self) -> &str {
        &self.client_id
    }
}

/// Mutual-TLS config for AWS IoT Core: device cert + key from disk, server
/// verified against the bundled webpki roots (which include Amazon Root CA 1).
fn tls_config(cfg: &MqttConfig) -> Result<rustls::ClientConfig, MqttError> {
    let cert_pem = std::fs::read(&cfg.cert_path)
        .map_err(|e| MqttError::Tls(format!("reading cert {}: {e}", cfg.cert_path)))?;
    let key_pem = std::fs::read(&cfg.key_path)
        .map_err(|e| MqttError::Tls(format!("reading key {}: {e}", cfg.key_path)))?;

    let certs = rustls_pemfile::certs(&mut cert_pem.as_slice())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| MqttError::Tls(format!("parsing cert PEM: {e}")))?;
    if certs.is_empty() {
        return Err(MqttError::Tls(format!("no certificate found in {}", cfg.cert_path)));
    }
    let key = rustls_pemfile::private_key(&mut key_pem.as_slice())
        .map_err(|e| MqttError::Tls(format!("parsing key PEM: {e}")))?
        .ok_or_else(|| MqttError::Tls(format!("no private key found in {}", cfg.key_path)))?;

    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    // Name the crypto provider explicitly: rumqttc's TLS stack also enables
    // aws-lc-rs, so rustls cannot pick a process default on its own.
    rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| MqttError::Tls(format!("selecting TLS versions: {e}")))?
        .with_root_certificates(roots)
        .with_client_auth_cert(certs, key)
        .map_err(|e| MqttError::Tls(format!("building TLS config: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn cfg(cert_path: &str, key_path: &str) -> MqttConfig {
        MqttConfig {
            endpoint: "localhost".into(),
            port: 8883,
            client_id: "test".into(),
            cert_path: cert_path.into(),
            key_path: key_path.into(),
        }
    }

    fn temp_with(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, "{content}").unwrap();
        f
    }

    fn tls_err(cfg: &MqttConfig) -> String {
        match RumqttcPublisher::connect(cfg) {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected TLS error"),
        }
    }

    #[test]
    fn connect_errors_on_missing_cert() {
        let err = tls_err(&cfg("/nonexistent/cert.pem", "/nonexistent/key.pem"));
        assert!(err.contains("reading cert"), "unexpected: {err}");
    }

    #[test]
    fn connect_errors_on_missing_key() {
        let cert = temp_with("not a cert");
        let err = tls_err(&cfg(cert.path().to_str().unwrap(), "/nonexistent/key.pem"));
        assert!(err.contains("reading key"), "unexpected: {err}");
    }

    #[test]
    fn connect_errors_when_cert_file_has_no_certificate() {
        let cert = temp_with("not a cert");
        let key = temp_with("not a key");
        let err = tls_err(&cfg(cert.path().to_str().unwrap(), key.path().to_str().unwrap()));
        assert!(err.contains("no certificate found"), "unexpected: {err}");
    }

    #[test]
    fn connect_errors_when_key_file_has_no_key() {
        let generated = rcgen::generate_simple_self_signed(vec!["pish".into()]).unwrap();
        let cert = temp_with(&generated.cert.pem());
        let key = temp_with("not a key");
        let err = tls_err(&cfg(cert.path().to_str().unwrap(), key.path().to_str().unwrap()));
        assert!(err.contains("no private key found"), "unexpected: {err}");
    }

    #[test]
    fn tls_config_accepts_valid_cert_and_key() {
        let generated = rcgen::generate_simple_self_signed(vec!["pish".into()]).unwrap();
        let cert = temp_with(&generated.cert.pem());
        let key = temp_with(&generated.key_pair.serialize_pem());
        let tls = tls_config(&cfg(cert.path().to_str().unwrap(), key.path().to_str().unwrap()))
            .expect("valid cert and key");
        assert!(tls.client_auth_cert_resolver.has_certs());
    }

    #[test]
    fn mqtt_error_display() {
        assert_eq!(MqttError::Publish("full".into()).to_string(), "MQTT publish: full");
        assert_eq!(MqttError::Tls("bad".into()).to_string(), "MQTT TLS setup: bad");
    }

    #[test]
    fn publisher_trait_object_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync + ?Sized>() {}
        assert_send_sync::<dyn MqttPublisher>();
    }
}
