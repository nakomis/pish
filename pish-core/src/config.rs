//! Runtime configuration for pish, loaded from a TOML file with environment
//! variable overrides — the same pattern as nakostat's sensor daemon.
//!
//! Lookup order for the file: `--config <path>`, then `$PISH_CONFIG`, then
//! [`DEFAULT_CONFIG_PATH`]. A missing file is not an error: pish still runs,
//! just without MQTT (handy for the macOS smoke test).

use serde::Deserialize;
use std::path::{Path, PathBuf};

pub const DEFAULT_CONFIG_PATH: &str = "/etc/pish/pish.toml";

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Config {
    /// Broker connection. `None` disables MQTT entirely.
    pub mqtt: Option<MqttConfig>,
    #[serde(default)]
    pub nakostat: NakostatConfig,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MqttConfig {
    /// AWS IoT Core ATS data endpoint, e.g. `abc123-ats.iot.eu-west-2.amazonaws.com`.
    pub endpoint: String,
    #[serde(default = "default_port")]
    pub port: u16,
    /// MQTT client id. AWS IoT policies pin this to the thing name.
    pub client_id: String,
    pub cert_path: String,
    pub key_path: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct NakostatConfig {
    /// Topic the thermostat widget publishes the setpoint to.
    #[serde(default = "default_setpoint_topic")]
    pub setpoint_topic: String,
}

impl Default for NakostatConfig {
    fn default() -> Self {
        Self { setpoint_topic: default_setpoint_topic() }
    }
}

fn default_port() -> u16 {
    8883
}

fn default_setpoint_topic() -> String {
    "nakostat/prod/pish/setpoint".to_string()
}

#[derive(Debug)]
pub enum ConfigError {
    Read(PathBuf, std::io::Error),
    Parse(PathBuf, toml::de::Error),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Read(p, e) => write!(f, "reading config file {}: {e}", p.display()),
            ConfigError::Parse(p, e) => write!(f, "parsing config file {}: {e}", p.display()),
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    /// Parse a TOML string and apply `PISH_*` environment overrides.
    pub fn from_toml_str(text: &str) -> Result<Self, toml::de::Error> {
        let mut cfg: Config = toml::from_str(text)?;
        cfg.apply_overrides(|k| std::env::var(k).ok());
        Ok(cfg)
    }

    /// Load from `path`. A missing file yields the default (MQTT disabled).
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut cfg = Config::default();
                cfg.apply_overrides(|k| std::env::var(k).ok());
                return Ok(cfg);
            }
            Err(e) => return Err(ConfigError::Read(path.to_path_buf(), e)),
        };
        Self::from_toml_str(&text).map_err(|e| ConfigError::Parse(path.to_path_buf(), e))
    }

    /// Load from the path given by `--config`, `$PISH_CONFIG` or the default.
    pub fn load() -> Result<Self, ConfigError> {
        let args: Vec<String> = std::env::args().collect();
        let path = config_path(&args, std::env::var("PISH_CONFIG").ok());
        Self::from_file(&path)
    }

    /// Apply environment overrides. `get` is injected so tests needn't touch
    /// the process-global environment.
    fn apply_overrides(&mut self, get: impl Fn(&str) -> Option<String>) {
        if let Some(v) = get("PISH_NAKOSTAT_SETPOINT_TOPIC") {
            self.nakostat.setpoint_topic = v;
        }

        // The endpoint alone is enough to switch MQTT on from the environment,
        // provided the remaining fields come from the file or the environment.
        if self.mqtt.is_none() {
            if let (Some(endpoint), Some(client_id), Some(cert_path), Some(key_path)) = (
                get("PISH_MQTT_ENDPOINT"),
                get("PISH_MQTT_CLIENT_ID"),
                get("PISH_MQTT_CERT_PATH"),
                get("PISH_MQTT_KEY_PATH"),
            ) {
                self.mqtt = Some(MqttConfig {
                    endpoint,
                    port: default_port(),
                    client_id,
                    cert_path,
                    key_path,
                });
            }
        }

        if let Some(mqtt) = self.mqtt.as_mut() {
            if let Some(v) = get("PISH_MQTT_ENDPOINT") {
                mqtt.endpoint = v;
            }
            if let Some(port) = get("PISH_MQTT_PORT").and_then(|v| v.parse().ok()) {
                mqtt.port = port;
            }
            if let Some(v) = get("PISH_MQTT_CLIENT_ID") {
                mqtt.client_id = v;
            }
            if let Some(v) = get("PISH_MQTT_CERT_PATH") {
                mqtt.cert_path = v;
            }
            if let Some(v) = get("PISH_MQTT_KEY_PATH") {
                mqtt.key_path = v;
            }
        }
    }
}

fn config_path(args: &[String], env: Option<String>) -> PathBuf {
    if let Some(pos) = args.iter().position(|a| a == "--config") {
        if let Some(p) = args.get(pos + 1) {
            return PathBuf::from(p);
        }
    }
    env.map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::Write;

    const FULL: &str = r#"
[mqtt]
endpoint  = "abc123-ats.iot.eu-west-2.amazonaws.com"
client_id = "nakostat-pish-prod"
cert_path = "/etc/pish/certs/cert.pem"
key_path  = "/etc/pish/certs/key.pem"

[nakostat]
setpoint_topic = "nakostat/sandbox/pish/setpoint"
"#;

    fn parse(text: &str) -> Config {
        toml::from_str(text).unwrap()
    }

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> =
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |k| map.get(k).cloned()
    }

    #[test]
    fn parses_full_config() {
        let cfg = parse(FULL);
        let mqtt = cfg.mqtt.unwrap();
        assert_eq!(mqtt.endpoint, "abc123-ats.iot.eu-west-2.amazonaws.com");
        assert_eq!(mqtt.port, 8883, "port defaults to 8883");
        assert_eq!(mqtt.client_id, "nakostat-pish-prod");
        assert_eq!(mqtt.cert_path, "/etc/pish/certs/cert.pem");
        assert_eq!(mqtt.key_path, "/etc/pish/certs/key.pem");
        assert_eq!(cfg.nakostat.setpoint_topic, "nakostat/sandbox/pish/setpoint");
    }

    #[test]
    fn empty_config_disables_mqtt_and_uses_default_topic() {
        let cfg = parse("");
        assert!(cfg.mqtt.is_none());
        assert_eq!(cfg.nakostat.setpoint_topic, "nakostat/prod/pish/setpoint");
    }

    #[test]
    fn mqtt_section_missing_required_field_is_an_error() {
        assert!(toml::from_str::<Config>("[mqtt]\nendpoint = \"x\"\n").is_err());
    }

    #[test]
    fn env_overrides_existing_mqtt_fields() {
        let mut cfg = parse(FULL);
        cfg.apply_overrides(env(&[
            ("PISH_MQTT_ENDPOINT", "other-ats.iot.eu-west-2.amazonaws.com"),
            ("PISH_MQTT_PORT", "443"),
            ("PISH_MQTT_CLIENT_ID", "nakostat-pish-sandbox"),
            ("PISH_MQTT_CERT_PATH", "/tmp/c.pem"),
            ("PISH_MQTT_KEY_PATH", "/tmp/k.pem"),
            ("PISH_NAKOSTAT_SETPOINT_TOPIC", "nakostat/test/pish/setpoint"),
        ]));
        let mqtt = cfg.mqtt.unwrap();
        assert_eq!(mqtt.endpoint, "other-ats.iot.eu-west-2.amazonaws.com");
        assert_eq!(mqtt.port, 443);
        assert_eq!(mqtt.client_id, "nakostat-pish-sandbox");
        assert_eq!(mqtt.cert_path, "/tmp/c.pem");
        assert_eq!(mqtt.key_path, "/tmp/k.pem");
        assert_eq!(cfg.nakostat.setpoint_topic, "nakostat/test/pish/setpoint");
    }

    #[test]
    fn invalid_port_override_is_ignored() {
        let mut cfg = parse(FULL);
        cfg.apply_overrides(env(&[("PISH_MQTT_PORT", "not-a-port")]));
        assert_eq!(cfg.mqtt.unwrap().port, 8883);
    }

    #[test]
    fn env_alone_can_enable_mqtt() {
        let mut cfg = Config::default();
        cfg.apply_overrides(env(&[
            ("PISH_MQTT_ENDPOINT", "e"),
            ("PISH_MQTT_CLIENT_ID", "c"),
            ("PISH_MQTT_CERT_PATH", "cp"),
            ("PISH_MQTT_KEY_PATH", "kp"),
        ]));
        let mqtt = cfg.mqtt.unwrap();
        assert_eq!(mqtt.endpoint, "e");
        assert_eq!(mqtt.port, 8883);
    }

    #[test]
    fn partial_env_does_not_enable_mqtt() {
        let mut cfg = Config::default();
        cfg.apply_overrides(env(&[("PISH_MQTT_ENDPOINT", "e")]));
        assert!(cfg.mqtt.is_none());
    }

    #[test]
    fn missing_file_yields_default() {
        let cfg = Config::from_file(Path::new("/nonexistent/pish.toml")).unwrap();
        assert_eq!(cfg.nakostat, NakostatConfig::default());
    }

    #[test]
    fn from_file_reads_and_parses() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, "{FULL}").unwrap();
        let cfg = Config::from_file(f.path()).unwrap();
        assert!(cfg.mqtt.is_some());
    }

    #[test]
    fn from_file_reports_parse_errors_with_path() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, "this is = = not toml").unwrap();
        let err = Config::from_file(f.path()).unwrap_err();
        assert!(matches!(err, ConfigError::Parse(..)));
        assert!(err.to_string().contains("parsing config file"));
    }

    #[test]
    fn from_file_reports_read_errors() {
        // A directory exists but cannot be read as a file.
        let dir = tempfile::tempdir().unwrap();
        let err = Config::from_file(dir.path()).unwrap_err();
        assert!(matches!(err, ConfigError::Read(..)));
        assert!(err.to_string().contains("reading config file"));
    }

    #[test]
    fn config_path_prefers_cli_arg() {
        let args = vec!["pish".into(), "--config".into(), "/a.toml".into()];
        assert_eq!(config_path(&args, Some("/b.toml".into())), PathBuf::from("/a.toml"));
    }

    #[test]
    fn config_path_falls_back_to_env_then_default() {
        let args = vec!["pish".to_string()];
        assert_eq!(config_path(&args, Some("/b.toml".into())), PathBuf::from("/b.toml"));
        assert_eq!(config_path(&args, None), PathBuf::from(DEFAULT_CONFIG_PATH));
    }

    #[test]
    fn config_path_ignores_dangling_flag() {
        let args = vec!["pish".into(), "--config".into()];
        assert_eq!(config_path(&args, None), PathBuf::from(DEFAULT_CONFIG_PATH));
    }
}
