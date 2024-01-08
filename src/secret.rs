use std::fs;
use std::path::{Path, PathBuf};

use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, Nonce, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, KeyInit};
use anyhow::{bail, Context, Result};
use pbkdf2::pbkdf2_hmac_array;
use sha2::Sha256;

use crate::utils;

enum Output {
    Encrypt(String),
    Decrypt(Vec<u8>),
}

impl Output {
    fn to_string(self) -> Result<String> {
        match self {
            Output::Encrypt(s) => Ok(s),
            Output::Decrypt(data) => String::from_utf8(data)
                .context("decode secret result as utf-8 failed, please consider write it to file"),
        }
    }

    fn to_vec(self) -> Vec<u8> {
        match self {
            Output::Encrypt(s) => s.into_bytes(),
            Output::Decrypt(data) => data,
        }
    }
}

pub struct Secret {
    data: Vec<u8>,
    cipher: bool,

    replace: Option<PathBuf>,
}

impl Secret {
    const SALT_LENGTH: usize = 5;
    const NONCE_LENGTH: usize = 12;
    const NONCE_OFFSET: usize = Self::SALT_LENGTH + Self::NONCE_LENGTH;

    const SECRET_BEGIN_LINE: &'static [u8] = b"-----BEGIN ROXIDE SECRET-----";
    const SECRET_END_LINE: &'static [u8] = b"-----END ROXIDE SECRET-----";

    const SECRET_LINE_MAX_LENGTH: usize = 70;

    const PBKDF2_ROUNDS: u32 = 600_000;

    pub fn read(path: impl AsRef<Path>, replace: bool) -> Result<Secret> {
        let replace = if replace {
            Some(PathBuf::from(path.as_ref()))
        } else {
            None
        };

        let data = fs::read(path.as_ref())
            .with_context(|| format!("read secret file '{}'", path.as_ref().display()))?;
        if !data.starts_with(Self::SECRET_BEGIN_LINE) {
            return Ok(Secret {
                data,
                cipher: false,
                replace,
            });
        }

        let content = String::from_utf8(data).context("decode secret file as utf-8")?;
        let lines = content.split('\n');

        let mut cipher_content = String::with_capacity(content.len());
        for line in lines {
            let line = line.trim();
            if line == "" {
                continue;
            }

            if line.as_bytes() == Self::SECRET_BEGIN_LINE {
                continue;
            }

            cipher_content.push_str(line);

            if line.as_bytes() == Self::SECRET_END_LINE {
                break;
            }
        }

        let data = hex::decode(cipher_content).context("invalid hex string in secret file")?;

        Ok(Secret {
            data,
            cipher: true,
            replace,
        })
    }

    pub fn write(&self, password: impl AsRef<str>) -> Result<Option<String>> {
        let output = if self.cipher {
            Output::Encrypt(self.wrap_encrypt(password)?)
        } else {
            Output::Decrypt(self.decrypt(password)?)
        };
        match self.replace.as_ref() {
            Some(path) => {
                let data = output.to_vec();
                utils::write_file(path, &data)?;
                Ok(None)
            }
            None => Ok(Some(output.to_string()?)),
        }
    }

    fn wrap_encrypt(&self, password: impl AsRef<str>) -> Result<String> {
        let cipher = self.encrypt(password)?;
        let mut wrap = String::with_capacity(
            Self::SECRET_BEGIN_LINE.len() + 1 + cipher.len() + Self::SECRET_END_LINE.len() + 1,
        );
        for ch in Self::SECRET_BEGIN_LINE {
            wrap.push(*ch as char);
        }
        wrap.push('\n');

        let mut line_length = 0;
        for ch in cipher.as_bytes() {
            if line_length > Self::SECRET_LINE_MAX_LENGTH {
                line_length = 0;
                wrap.push('\n');
            }
            wrap.push(*ch as char);
            line_length += 1;
        }

        for ch in Self::SECRET_END_LINE {
            wrap.push(*ch as char);
        }
        wrap.push('\n');

        Ok(wrap)
    }

    fn encrypt(&self, password: impl AsRef<str>) -> Result<String> {
        let mut salt: [u8; Self::SALT_LENGTH] = [0; Self::SALT_LENGTH];
        let mut rng = OsRng::default();
        rng.fill_bytes(&mut salt);

        let key = pbkdf2_hmac_array::<Sha256, 32>(
            password.as_ref().as_bytes(),
            &salt,
            Self::PBKDF2_ROUNDS,
        );
        let key = Key::<Aes256Gcm>::from_slice(&key);

        let cipher = Aes256Gcm::new(&key);
        let nonce = Aes256Gcm::generate_nonce(&mut rng);
        assert_eq!(nonce.len(), Self::NONCE_LENGTH);

        let cipher_data = match cipher.encrypt(&nonce, self.data.as_ref()) {
            Ok(data) => data,
            Err(err) => bail!("use aes256gcm to encrypt data: {err}"),
        };

        let mut result =
            Vec::with_capacity(Self::SALT_LENGTH + Self::NONCE_LENGTH + cipher_data.len());
        result.extend(salt.to_vec());
        result.extend(nonce.to_vec());
        result.extend(cipher_data);

        Ok(hex::encode(result))
    }

    fn decrypt(&self, password: impl AsRef<str>) -> Result<Vec<u8>> {
        if self.data.len() < Self::SALT_LENGTH + Self::NONCE_LENGTH + 1 {
            bail!("invalid secret data, too short");
        }

        let salt = &self.data[..Self::SALT_LENGTH];
        let nonce = &self.data[Self::SALT_LENGTH..Self::NONCE_OFFSET];
        let cipher_data = &self.data[Self::NONCE_OFFSET..];

        let key = pbkdf2_hmac_array::<Sha256, 32>(
            password.as_ref().as_bytes(),
            salt,
            Self::PBKDF2_ROUNDS,
        );
        let key = Key::<Aes256Gcm>::from_slice(&key);

        let cipher = Aes256Gcm::new(&key);

        let nonce = Nonce::<Aes256Gcm>::from_slice(nonce);
        let plain = match cipher.decrypt(&nonce, cipher_data) {
            Ok(data) => data,
            Err(_) => bail!("decrypt secret failed, incorrect password or content"),
        };

        Ok(plain)
    }
}
