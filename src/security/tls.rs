// TLS 1.3設定と管理
//
// rustlsを使用したTLS 1.3クライアント・サーバー設定を提供します。
// 相互TLS認証と証明書検証ロジックを実装します。

use std::io::Cursor;
use std::sync::Arc;

use rustls::{
    Certificate, ClientConfig, RootCertStore, ServerConfig,
    PrivateKey, SupportedCipherSuite, ALL_CIPHER_SUITES,
};
use rustls_pemfile::{certs, pkcs8_private_keys, rsa_private_keys};
use tokio_rustls::{TlsAcceptor, TlsConnector};

/// TLS関連のエラー
#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    #[error("TLS configuration error: {message}")]
    Configuration { message: String },
    
    #[error("Certificate error: {message}")]
    Certificate { message: String },
    
    #[error("Private key error: {message}")]
    PrivateKey { message: String },
    
    #[error("TLS handshake error: {message}")]
    Handshake { message: String },
    
    #[error("Certificate verification error: {message}")]
    Verification { message: String },
    
    #[error("File I/O error: {message}")]
    FileOperation { message: String },
}

/// TLSの結果型
pub type TlsResult<T> = Result<T, TlsError>;

/// TLS設定
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TlsConfig {
    /// サーバー証明書ファイルパス
    pub cert_file: Option<String>,
    
    /// サーバー秘密鍵ファイルパス
    pub key_file: Option<String>,
    
    /// CA証明書ファイルパス
    pub ca_cert_file: Option<String>,
    
    /// クライアント証明書検証を有効にするか
    pub require_client_cert: bool,
    
    /// サーバー証明書検証を有効にするか
    pub verify_server_cert: bool,
    
    /// サポートするTLSバージョン
    pub min_tls_version: String,
    
    /// 許可する暗号スイート
    pub cipher_suites: Vec<String>,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            cert_file: None,
            key_file: None,
            ca_cert_file: None,
            require_client_cert: false,
            verify_server_cert: true,
            min_tls_version: "1.3".to_string(),
            cipher_suites: vec![
                "TLS13_AES_256_GCM_SHA384".to_string(),
                "TLS13_AES_128_GCM_SHA256".to_string(),
                "TLS13_CHACHA20_POLY1305_SHA256".to_string(),
            ],
        }
    }
}

/// TLSクライアント設定
pub struct TlsClientConfig {
    config: Arc<ClientConfig>,
}

impl TlsClientConfig {
    /// 新しいTLSクライアント設定を作成
    pub fn new(tls_config: &TlsConfig) -> TlsResult<Self> {
        let mut root_store = RootCertStore::empty();
        
        // CA証明書が指定されている場合は追加
        if let Some(ca_cert_file) = &tls_config.ca_cert_file {
            let ca_certs = load_certificates(ca_cert_file)?;
            for cert in ca_certs {
                root_store.add(&cert)
                    .map_err(|e| TlsError::Certificate {
                        message: format!("Failed to add CA certificate: {}", e)
                    })?;
            }
        } else {
            // システムのCA証明書を使用
            root_store.add_trust_anchors(
                webpki_roots::TLS_SERVER_ROOTS.iter().map(|ta| {
                    rustls::OwnedTrustAnchor::from_subject_spki_name_constraints(
                        ta.subject,
                        ta.spki,
                        ta.name_constraints,
                    )
                })
            );
        }
        
        let config = ClientConfig::builder()
            .with_cipher_suites(&get_cipher_suites(&tls_config.cipher_suites)?)
            .with_safe_default_kx_groups()
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(|e| TlsError::Configuration {
                message: format!("Failed to create TLS client config: {}", e)
            })?
            .with_root_certificates(root_store);
        
        // クライアント証明書が指定されている場合は設定
        let config = if let (Some(cert_file), Some(key_file)) = (&tls_config.cert_file, &tls_config.key_file) {
            let certs = load_certificates(cert_file)?;
            let key = load_private_key(key_file)?;
            
            config.with_client_auth_cert(certs, key)
                .map_err(|e| TlsError::Configuration {
                    message: format!("Failed to configure client certificate: {}", e)
                })?
        } else {
            config.with_no_client_auth()
        };
        
        Ok(Self {
            config: Arc::new(config),
        })
    }
    
    /// TLSコネクターを取得
    pub fn connector(&self) -> TlsConnector {
        TlsConnector::from(self.config.clone())
    }
    
    /// 設定を取得
    pub fn config(&self) -> Arc<ClientConfig> {
        self.config.clone()
    }
}

/// TLSサーバー設定
pub struct TlsServerConfig {
    config: Arc<ServerConfig>,
}

impl TlsServerConfig {
    /// 新しいTLSサーバー設定を作成
    pub fn new(tls_config: &TlsConfig) -> TlsResult<Self> {
        let cert_file = tls_config.cert_file.as_ref()
            .ok_or_else(|| TlsError::Configuration {
                message: "Server certificate file is required".to_string()
            })?;
        
        let key_file = tls_config.key_file.as_ref()
            .ok_or_else(|| TlsError::Configuration {
                message: "Server private key file is required".to_string()
            })?;
        
        let certs = load_certificates(cert_file)?;
        let key = load_private_key(key_file)?;
        
        let config = ServerConfig::builder()
            .with_cipher_suites(&get_cipher_suites(&tls_config.cipher_suites)?)
            .with_safe_default_kx_groups()
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(|e| TlsError::Configuration {
                message: format!("Failed to create TLS server config: {}", e)
            })?;
        
        // クライアント証明書検証設定
        let config = if tls_config.require_client_cert {
            if let Some(ca_cert_file) = &tls_config.ca_cert_file {
                let mut root_store = RootCertStore::empty();
                let ca_certs = load_certificates(ca_cert_file)?;
                for cert in ca_certs {
                    root_store.add(&cert)
                        .map_err(|e| TlsError::Certificate {
                            message: format!("Failed to add CA certificate: {}", e)
                        })?;
                }
                
                config.with_client_cert_verifier(
                    std::sync::Arc::new(rustls::server::AllowAnyAuthenticatedClient::new(root_store))
                )
            } else {
                return Err(TlsError::Configuration {
                    message: "CA certificate file is required when client certificate verification is enabled".to_string()
                });
            }
        } else {
            config.with_no_client_auth()
        };
        
        let config = config.with_single_cert(certs, key)
            .map_err(|e| TlsError::Configuration {
                message: format!("Failed to configure server certificate: {}", e)
            })?;
        
        Ok(Self {
            config: Arc::new(config),
        })
    }
    
    /// TLSアクセプターを取得
    pub fn acceptor(&self) -> TlsAcceptor {
        TlsAcceptor::from(self.config.clone())
    }
    
    /// 設定を取得
    pub fn config(&self) -> Arc<ServerConfig> {
        self.config.clone()
    }
}

/// 証明書ファイルを読み込み
fn load_certificates(cert_file: &str) -> TlsResult<Vec<Certificate>> {
    let cert_data = std::fs::read(cert_file)
        .map_err(|e| TlsError::FileOperation {
            message: format!("Failed to read certificate file '{}': {}", cert_file, e)
        })?;
    
    let mut cursor = Cursor::new(cert_data);
    let certs = certs(&mut cursor)
        .map_err(|e| TlsError::Certificate {
            message: format!("Failed to parse certificates: {}", e)
        })?;
    
    if certs.is_empty() {
        return Err(TlsError::Certificate {
            message: "No certificates found in file".to_string()
        });
    }
    
    Ok(certs.into_iter().map(Certificate).collect())
}

/// 秘密鍵ファイルを読み込み
fn load_private_key(key_file: &str) -> TlsResult<PrivateKey> {
    let key_data = std::fs::read(key_file)
        .map_err(|e| TlsError::FileOperation {
            message: format!("Failed to read private key file '{}': {}", key_file, e)
        })?;
    
    let mut cursor = Cursor::new(key_data.clone());
    
    // PKCS8形式の秘密鍵を試行
    if let Ok(mut keys) = pkcs8_private_keys(&mut cursor) {
        if !keys.is_empty() {
            return Ok(PrivateKey(keys.remove(0)));
        }
    }
    
    // RSA形式の秘密鍵を試行
    let mut cursor = Cursor::new(key_data);
    if let Ok(mut keys) = rsa_private_keys(&mut cursor) {
        if !keys.is_empty() {
            return Ok(PrivateKey(keys.remove(0)));
        }
    }
    
    Err(TlsError::PrivateKey {
        message: "No valid private key found in file".to_string()
    })
}

/// 暗号スイートを取得
fn get_cipher_suites(suite_names: &[String]) -> TlsResult<Vec<SupportedCipherSuite>> {
    let mut suites = Vec::new();
    
    for name in suite_names {
        let suite = match name.as_str() {
            "TLS13_AES_256_GCM_SHA384" => rustls::cipher_suite::TLS13_AES_256_GCM_SHA384,
            "TLS13_AES_128_GCM_SHA256" => rustls::cipher_suite::TLS13_AES_128_GCM_SHA256,
            "TLS13_CHACHA20_POLY1305_SHA256" => rustls::cipher_suite::TLS13_CHACHA20_POLY1305_SHA256,
            _ => return Err(TlsError::Configuration {
                message: format!("Unsupported cipher suite: {}", name)
            })
        };
        suites.push(suite);
    }
    
    if suites.is_empty() {
        // デフォルトのTLS 1.3暗号スイートを使用
        suites = ALL_CIPHER_SUITES.iter()
            .filter(|suite| suite.version() == &rustls::version::TLS13)
            .copied()
            .collect();
    }
    
    Ok(suites)
}

/// 自己署名証明書とキーペアを生成（テスト用）
#[cfg(feature = "test-utils")]
pub fn generate_self_signed_cert() -> TlsResult<(Vec<u8>, Vec<u8>)> {
    use rcgen::{Certificate, CertificateParams, DistinguishedName};
    use time::OffsetDateTime;
    
    let mut params = CertificateParams::new(vec!["localhost".to_string()]);
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(rcgen::DnType::CommonName, "Conduit Test");
    params.not_before = OffsetDateTime::now_utc() - time::Duration::days(1);
    params.not_after = OffsetDateTime::now_utc() + time::Duration::days(365);
    
    let cert = Certificate::from_params(params)
        .map_err(|e| TlsError::Certificate {
            message: format!("Failed to generate self-signed certificate: {}", e)
        })?;
    
    let cert_pem = cert.serialize_pem()
        .map_err(|e| TlsError::Certificate {
            message: format!("Failed to serialize certificate: {}", e)
        })?;
    
    let key_pem = cert.serialize_private_key_pem();
    
    Ok((cert_pem.into_bytes(), key_pem.into_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    // テスト用の証明書とキーを作成
    fn create_test_cert_files() -> (NamedTempFile, NamedTempFile) {
        let cert_pem = r#"-----BEGIN CERTIFICATE-----
MIIDCTCCAfGgAwIBAgIULij+FrIWmNeMhNtXy9c8w7ZsgTMwDQYJKoZIhvcNAQEL
BQAwFDESMBAGA1UEAwwJbG9jYWxob3N0MB4XDTI1MDYxNDE1NTgxNFoXDTI2MDYx
NDE1NTgxNFowFDESMBAGA1UEAwwJbG9jYWxob3N0MIIBIjANBgkqhkiG9w0BAQEF
AAOCAQ8AMIIBCgKCAQEAvgMhJ5KbEJVQvCmFeREQ1xV6MCZtv0JiYwfwANHYgQcf
Jv5KehuC357k+5q2wYRWim/vFQXS2x4xwc1Vmbg6R3SjqJtEguP3gHykrexvcSVU
hgHpIXl6mT0A3njyCiYWT32cFtYaeL006Me3w99pLREysMz6kosYpuHBXo8W5Wvv
oY7ab1Ngj58vO98sT99KRBaXxe0dCom2/g0mVgzugjKONgfPFxGxzjmCozsDVya2
bNbUXYkv6iCe5e+gt9atclu35GeQ6rxJrki9U+AG9v/ujDaG4sq+UO0ReBPk6/Ho
G1lwhxAAmBQRCdT+FybiEVEh4Y9o6h4UOXLGZob38QIDAQABo1MwUTAdBgNVHQ4E
FgQUopkPHaXr7rW0xUabwUWGgsrX2zowHwYDVR0jBBgwFoAUopkPHaXr7rW0xUab
wUWGgsrX2zowDwYDVR0TAQH/BAUwAwEB/zANBgkqhkiG9w0BAQsFAAOCAQEAtmaT
x3m0KxBr+2BZjFj4PgaqmNHa925d7uAQLnJ+IKczqw28vchKeuArQcay1TNImDMa
zGL6/K9npq+ZOaQjhLNsOlN2hcUT5FukXk6fSILpwkrUxNqUKZOVl7Mz2OD5M91j
0oG8DzlFtwgamuX9+uckV+40vVnnKVk4mEaf1sKc/RQ1NP2rN6urFlmu3W4QSmeT
tcaYSiUHZmUhHBK4bhz79D6ajzqHuzP34WVDW56Efe0HO2d6vL7AKUYMZTeNOOvW
s1BqB3Ty4RbAEGkvH9wbtqVrK6uWUge39NVZqzlsCdSXr+1Zr1KWkvRdHckmkKT1
kW6bflsLJJ4bD4Srog==
-----END CERTIFICATE-----
"#;

        let key_pem = r#"-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC+AyEnkpsQlVC8
KYV5ERDXFXowJm2/QmJjB/AA0diBBx8m/kp6G4LfnuT7mrbBhFaKb+8VBdLbHjHB
zVWZuDpHdKOom0SC4/eAfKSt7G9xJVSGAekheXqZPQDeePIKJhZPfZwW1hp4vTTo
x7fD32ktETKwzPqSixim4cFejxbla++hjtpvU2CPny873yxP30pEFpfF7R0Kibb+
DSZWDO6CMo42B88XEbHOOYKjOwNXJrZs1tRdiS/qIJ7l76C31q1yW7fkZ5DqvEmu
SL1T4Ab2/+6MNobiyr5Q7RF4E+Tr8egbWXCHEACYFBEJ1P4XJuIRUSHhj2jqHhQ5
csZmhvfxAgMBAAECggEAChirzqjT7n9QnpRCOwWfdHOis3jx70NoFkUEtAvHyktb
lLyBrpf++JeT0+lg+Uq+jR/s1Jwjm8Wwn9Cjp3BflbkVPSSgRlLgrYYcpf/giA2T
aS95eMWLWyXKVwfsLIKLlZu3YItDNX6Fl4eNNIOO2M3czgIw9Ppzy+JGXm+R/Yy1
k+2pS2EVageAcdHG9EPJKRTVDlyRO6U4H73y1PkuVllxVmLWlnw05iXWsDIiqpUd
+/oCKKUzcx0/xhRBcsTkbC2f1/cGw5cFfptp99tXzSpziESc8r9bF6ry7N2HFkHl
Z9n+fZiAqGFsqRnuHu5Fvr0mJuTerbAXrW3GdbpRMQKBgQDh9EUWIMLSO1gE6gBg
/B9sJICO/5wNB/sC9uqeEizgvRxyzdznVqSoh6JVrEbuMkS21MWa4Usc4f+oLYQw
9pWG0ULtJKe2vZxwQ5Jw81EG8tXGa2dS6v4JzO9v1me+dB3ug03bSwRzIr0cvZ05
oCUIdMoalIvNpQCfTQdvwey1TQKBgQDXR1sTQgPXS7UhvnuqRG2+8FEwzQ8A2blV
cVtwWrZsDJtpLDdhS6CdYVgpaLFEwLlHb0rGqFMLF/TW0NL6mKydOgCas3tuQvMG
KkWI4j0hc8PGJ5xp2gdU/qhvJCBa02cy+lQXbvEX6w3ODmw3LmuJs3Jz4LO0LLmr
5eqyf5erNQKBgB/88uLlFG+q0xUvA4q3ZsNqj4CvEyqNHK4YwrJKJGxUmXhvSHy/
uWPaf9LdJtWF7FBt4uPATV/zZrwqZXKBFj4V1gG4lL28Li9+gXbYG3p+WmFSxzfn
6Lr+dk9A7UL7MVmY4SbQgfrCyj8NYPrNfzCpSAtnSDt+xo3G2+yoaZhpAoGBAMrJ
VotPdhrKH/eHXSpIhzCDY5easfVVInXP8Bhn+srRHVv24Lvv534YglmPWtyuxlj6
BvAXAxLCIEpyLysLwh3g00zmoPB+dDpse0+z/W2ThOkVywsS7DEuKS94Uln8JYwp
rijcwpKLQlfF0j4wpe/yb2+kTOL99nBshaZa9l25AoGAdW7RXQcQ4JEHs61USI8I
077TsTl9ZIjOWKemnDKmW0AzrWGo0Rm6MFz3V/4lgnibIkAwZp/jeieP5fcFHvFF
4u5qq80PQ2K+I1jGRpgtv2GbZbPb66q9O1WuXN1dT5tc19xD8Rtz/WvNGX0iI1AV
ux05aJ6tQivRKUbJBYvuwRA=
-----END PRIVATE KEY-----"#;

        let mut cert_file = NamedTempFile::new().unwrap();
        cert_file.write_all(cert_pem.as_bytes()).unwrap();
        cert_file.flush().unwrap();

        let mut key_file = NamedTempFile::new().unwrap();
        key_file.write_all(key_pem.as_bytes()).unwrap();
        key_file.flush().unwrap();

        (cert_file, key_file)
    }

    #[test]
    fn test_tls_config_default() {
        let config = TlsConfig::default();
        assert_eq!(config.min_tls_version, "1.3");
        assert!(!config.require_client_cert);
        assert!(config.verify_server_cert);
        assert_eq!(config.cipher_suites.len(), 3);
    }

    #[test]
    fn test_cipher_suite_parsing() {
        let suite_names = vec![
            "TLS13_AES_256_GCM_SHA384".to_string(),
            "TLS13_AES_128_GCM_SHA256".to_string(),
        ];
        
        let suites = get_cipher_suites(&suite_names).unwrap();
        assert_eq!(suites.len(), 2);
    }

    #[test]
    fn test_invalid_cipher_suite() {
        let suite_names = vec!["INVALID_CIPHER_SUITE".to_string()];
        let result = get_cipher_suites(&suite_names);
        assert!(result.is_err());
    }
}