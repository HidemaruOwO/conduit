// Ed25519暗号化・デジタル署名実装
//
// Ed25519デジタル署名アルゴリズムの実装を提供します。
// 32バイトキーで128bit相当のセキュリティレベルを実現します。

use std::fmt;
use std::fs;
use std::path::Path;

use ed25519_dalek::{
    SigningKey, VerifyingKey, Signature, Signer, Verifier,
    PUBLIC_KEY_LENGTH, SECRET_KEY_LENGTH, SIGNATURE_LENGTH,
};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use base64::Engine;

#[derive(Debug, thiserror::Error)]
pub enum Ed25519Error {
    #[error("Key generation error: {message}")]
    KeyGeneration { message: String },
    
    #[error("Signing failed: {message}")]
    SigningFailed { message: String },
    
    #[error("Signature verification failed: {message}")]
    VerificationFailed { message: String },
    
    #[error("Key parsing error: {message}")]
    KeyParsing { message: String },
    
    #[error("File I/O error: {message}")]
    FileOperation { message: String },
    
    #[error("Encoding error: {message}")]
    Encoding { message: String },
}

/// Ed25519の結果型
pub type Ed25519Result<T> = Result<T, Ed25519Error>;

/// Ed25519キーペア
#[derive(Clone)]
pub struct Ed25519KeyPair {
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

impl Ed25519KeyPair {
    /// 新しいキーペアを生成
    pub fn generate() -> Ed25519Result<Self> {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();
        
        Ok(Self { signing_key, verifying_key })
    }
    
    /// 秘密鍵から既存のキーペアを復元
    pub fn from_secret_key_bytes(secret_bytes: &[u8]) -> Ed25519Result<Self> {
        if secret_bytes.len() != SECRET_KEY_LENGTH {
            return Err(Ed25519Error::KeyParsing {
                message: format!("Invalid secret key length: {} bytes (expected: {} bytes)",
                               secret_bytes.len(), SECRET_KEY_LENGTH)
            });
        }
        
        let signing_key = SigningKey::from_bytes(secret_bytes.try_into().map_err(|_| {
            Ed25519Error::KeyParsing {
                message: "Failed to convert secret key bytes to array".to_string()
            }
        })?);
        
        let verifying_key = signing_key.verifying_key();
        
        Ok(Self { signing_key, verifying_key })
    }
    
    /// Base64エンコードされた秘密鍵から復元
    pub fn from_base64_secret_key(secret_base64: &str) -> Ed25519Result<Self> {
        let secret_bytes = base64::engine::general_purpose::STANDARD.decode(secret_base64)
            .map_err(|e| Ed25519Error::Encoding {
                message: format!("Base64 decode failed: {}", e)
            })?;
        
        Self::from_secret_key_bytes(&secret_bytes)
    }
    
    /// ファイルから秘密鍵を読み込んでキーペアを復元
    pub fn from_file<P: AsRef<Path>>(secret_key_path: P) -> Ed25519Result<Self> {
        let content = fs::read_to_string(&secret_key_path)
            .map_err(|e| Ed25519Error::FileOperation {
                message: format!("Failed to read secret key file: {}", e)
            })?;
        
        let secret_base64 = content.trim();
        Self::from_base64_secret_key(secret_base64)
    }
    
    /// 公開鍵を取得
    pub fn public_key(&self) -> &VerifyingKey {
        &self.verifying_key
    }
    
    /// 公開鍵をバイト配列として取得
    pub fn public_key_bytes(&self) -> [u8; PUBLIC_KEY_LENGTH] {
        self.verifying_key.to_bytes()
    }
    
    /// 公開鍵をBase64文字列として取得
    pub fn public_key_base64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(self.public_key_bytes())
    }
    
    /// 秘密鍵をバイト配列として取得
    pub fn secret_key_bytes(&self) -> [u8; SECRET_KEY_LENGTH] {
        self.signing_key.to_bytes()
    }
    
    /// 秘密鍵をBase64文字列として取得
    pub fn secret_key_base64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(self.secret_key_bytes())
    }
    
    /// データに署名する
    pub fn sign(&self, data: &[u8]) -> Ed25519Result<Ed25519Signature> {
        let signature = self.signing_key.sign(data);
        Ok(Ed25519Signature { signature })
    }
    
    /// 署名を検証する
    pub fn verify(&self, data: &[u8], signature: &Ed25519Signature) -> Ed25519Result<bool> {
        match self.verifying_key.verify(data, &signature.signature) {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }
    
    /// 秘密鍵をファイルに保存
    pub fn save_secret_key<P: AsRef<Path>>(&self, path: P) -> Ed25519Result<()> {
        let secret_base64 = self.secret_key_base64();
        
        // ディレクトリが存在しない場合は作成
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)
                .map_err(|e| Ed25519Error::FileOperation {
                    message: format!("Failed to create directory: {}", e)
                })?;
        }
        
        fs::write(&path, secret_base64)
            .map_err(|e| Ed25519Error::FileOperation {
                message: format!("Failed to save secret key file: {}", e)
            })?;
        
        Ok(())
    }
    
    /// 公開鍵をファイルに保存
    pub fn save_public_key<P: AsRef<Path>>(&self, path: P) -> Ed25519Result<()> {
        let public_base64 = self.public_key_base64();
        
        // ディレクトリが存在しない場合は作成
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)
                .map_err(|e| Ed25519Error::FileOperation {
                    message: format!("Failed to create directory: {}", e)
                })?;
        }
        
        fs::write(&path, public_base64)
            .map_err(|e| Ed25519Error::FileOperation {
                message: format!("Failed to save public key file: {}", e)
            })?;
        
        Ok(())
    }
}

impl fmt::Debug for Ed25519KeyPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ed25519KeyPair")
            .field("public_key", &self.public_key_base64())
            .field("secret_key", &"[REDACTED]")
            .finish()
    }
}

/// Ed25519署名
#[derive(Clone, PartialEq, Eq)]
pub struct Ed25519Signature {
    signature: Signature,
}

impl Ed25519Signature {
    /// バイト配列から署名を復元
    pub fn from_bytes(bytes: &[u8]) -> Ed25519Result<Self> {
        if bytes.len() != SIGNATURE_LENGTH {
            return Err(Ed25519Error::KeyParsing {
                message: format!("Invalid signature length: {} bytes (expected: {} bytes)",
                               bytes.len(), SIGNATURE_LENGTH)
            });
        }
        
        let signature = Signature::from_bytes(bytes.try_into().map_err(|_| {
            Ed25519Error::KeyParsing {
                message: "Failed to convert signature bytes to array".to_string()
            }
        })?);
        
        Ok(Self { signature })
    }
    
    /// Base64文字列から署名を復元
    pub fn from_base64(signature_base64: &str) -> Ed25519Result<Self> {
        let bytes = base64::engine::general_purpose::STANDARD.decode(signature_base64)
            .map_err(|e| Ed25519Error::Encoding {
                message: format!("Base64 decode failed: {}", e)
            })?;
        
        Self::from_bytes(&bytes)
    }
    
    /// 署名をバイト配列として取得
    pub fn to_bytes(&self) -> [u8; SIGNATURE_LENGTH] {
        self.signature.to_bytes()
    }
    
    /// 署名をBase64文字列として取得
    pub fn to_base64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(self.to_bytes())
    }
}

impl Serialize for Ed25519Signature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_base64().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Ed25519Signature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let base64_str = String::deserialize(deserializer)?;
        Self::from_base64(&base64_str).map_err(serde::de::Error::custom)
    }
}

impl fmt::Debug for Ed25519Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ed25519Signature")
            .field("signature", &self.to_base64())
            .finish()
    }
}

/// 公開鍵による署名検証
pub fn verify_signature(
    public_key_bytes: &[u8],
    data: &[u8],
    signature: &Ed25519Signature,
) -> Ed25519Result<bool> {
    if public_key_bytes.len() != PUBLIC_KEY_LENGTH {
        return Err(Ed25519Error::KeyParsing {
            message: format!("Invalid public key length: {} bytes (expected: {} bytes)",
                           public_key_bytes.len(), PUBLIC_KEY_LENGTH)
        });
    }
    
    let public_key = VerifyingKey::from_bytes(public_key_bytes.try_into().map_err(|_| {
        Ed25519Error::KeyParsing {
            message: "Failed to convert public key bytes to array".to_string()
        }
    })?)
    .map_err(|e| Ed25519Error::KeyParsing {
        message: format!("Failed to parse public key: {}", e)
    })?;
    
    match public_key.verify(data, &signature.signature) {
        Ok(()) => Ok(true),
        Err(_) => Ok(false),
    }
}

/// セキュアな乱数生成
pub fn generate_random_bytes(length: usize) -> Vec<u8> {
    use rand::RngCore;
    let mut bytes = vec![0u8; length];
    OsRng.fill_bytes(&mut bytes);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_keypair_generation() {
        let keypair = Ed25519KeyPair::generate().unwrap();
        assert_eq!(keypair.public_key_bytes().len(), PUBLIC_KEY_LENGTH);
        assert_eq!(keypair.secret_key_bytes().len(), SECRET_KEY_LENGTH);
    }

    #[test]
    fn test_sign_and_verify() {
        let keypair = Ed25519KeyPair::generate().unwrap();
        let data = b"test message";
        
        let signature = keypair.sign(data).unwrap();
        let is_valid = keypair.verify(data, &signature).unwrap();
        assert!(is_valid);
        
        // 異なるデータで検証失敗をテスト
        let wrong_data = b"wrong message";
        let is_invalid = keypair.verify(wrong_data, &signature).unwrap();
        assert!(!is_invalid);
    }

    #[test]
    fn test_keypair_serialization() {
        let keypair = Ed25519KeyPair::generate().unwrap();
        let secret_base64 = keypair.secret_key_base64();
        let public_base64 = keypair.public_key_base64();
        
        let restored_keypair = Ed25519KeyPair::from_base64_secret_key(&secret_base64).unwrap();
        assert_eq!(public_base64, restored_keypair.public_key_base64());
    }

    #[test]
    fn test_file_operations() {
        let dir = tempdir().unwrap();
        let secret_path = dir.path().join("test.key");
        let public_path = dir.path().join("test.pub");
        
        let keypair = Ed25519KeyPair::generate().unwrap();
        let original_public = keypair.public_key_base64();
        
        // ファイルに保存
        keypair.save_secret_key(&secret_path).unwrap();
        keypair.save_public_key(&public_path).unwrap();
        
        // ファイルから復元
        let restored_keypair = Ed25519KeyPair::from_file(&secret_path).unwrap();
        assert_eq!(original_public, restored_keypair.public_key_base64());
    }

    #[test]
    fn test_signature_serialization() {
        let keypair = Ed25519KeyPair::generate().unwrap();
        let data = b"test data";
        
        let signature = keypair.sign(data).unwrap();
        let signature_base64 = signature.to_base64();
        
        let restored_signature = Ed25519Signature::from_base64(&signature_base64).unwrap();
        assert_eq!(signature.to_bytes(), restored_signature.to_bytes());
        
        let is_valid = keypair.verify(data, &restored_signature).unwrap();
        assert!(is_valid);
    }

    #[test]
    fn test_public_key_verification() {
        let keypair = Ed25519KeyPair::generate().unwrap();
        let data = b"verification test";
        
        let signature = keypair.sign(data).unwrap();
        let public_key_bytes = keypair.public_key_bytes();
        
        let is_valid = verify_signature(&public_key_bytes, data, &signature).unwrap();
        assert!(is_valid);
    }

    #[test]
    fn test_random_bytes_generation() {
        let bytes1 = generate_random_bytes(32);
        let bytes2 = generate_random_bytes(32);
        
        assert_eq!(bytes1.len(), 32);
        assert_eq!(bytes2.len(), 32);
        assert_ne!(bytes1, bytes2); // 異なる乱数が生成されることを確認
    }
}