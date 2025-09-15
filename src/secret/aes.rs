use std::io::{Read, Write};

use aes_gcm::aead::{Aead, Nonce, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, KeyInit};
use anyhow::{Context, Result, bail};
use pbkdf2::pbkdf2_hmac_array;
use rand::RngCore;
use sha2::Sha256;

pub struct AesCipher {
    cipher: Aes256Gcm,
    nonce: Nonce<Aes256Gcm>,
    pub salt_data: Vec<u8>,
    pub nonce_data: Vec<u8>,
}

impl AesCipher {
    const SALT_LENGTH: usize = 20;
    const NONCE_LENGTH: usize = 12;
    #[cfg(not(test))]
    const PBKDF2_ROUNDS: u32 = 600_000;
    #[cfg(test)]
    const PBKDF2_ROUNDS: u32 = 600;

    pub fn new(password: &str) -> Self {
        let mut rng = rand::rng();
        let mut salt = [0u8; Self::SALT_LENGTH];
        rng.fill_bytes(&mut salt);

        let mut rng = OsRng;
        let nonce = Aes256Gcm::generate_nonce(&mut rng);

        let key: [u8; 32] =
            pbkdf2_hmac_array::<Sha256, 32>(password.as_bytes(), &salt, Self::PBKDF2_ROUNDS);
        let key = Key::<Aes256Gcm>::from_slice(&key);
        let cipher = Aes256Gcm::new(key);

        Self {
            cipher,
            nonce,
            salt_data: salt.to_vec(),
            nonce_data: nonce.to_vec(),
        }
    }

    pub fn read<R>(mut src: R, password: &str) -> Result<Self>
    where
        R: Read,
    {
        let mut head = vec![0u8; Self::SALT_LENGTH + Self::NONCE_LENGTH];
        let n = src
            .read(&mut head)
            .context("read head from encrypted data")?;
        if n != Self::SALT_LENGTH + Self::NONCE_LENGTH {
            bail!("invalid encrypted data, head is incorrect");
        }

        let salt = &head[..Self::SALT_LENGTH];
        let nonce = &head[Self::SALT_LENGTH..];

        let key: [u8; 32] =
            pbkdf2_hmac_array::<Sha256, 32>(password.as_bytes(), salt, Self::PBKDF2_ROUNDS);
        let key = Key::<Aes256Gcm>::from_slice(&key);
        let cipher = Aes256Gcm::new(key);

        let nonce = Nonce::<Aes256Gcm>::from_slice(nonce);

        Ok(Self {
            cipher,
            nonce: *nonce,
            salt_data: salt.to_vec(),
            nonce_data: nonce.to_vec(),
        })
    }

    pub fn write_head<W: Write>(&self, dest: &mut W) -> Result<()> {
        dest.write_all(&self.salt_data)
            .context("write salt to destination")?;
        dest.write_all(&self.nonce_data)
            .context("write nonce to destination")?;
        Ok(())
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        match self.cipher.encrypt(&self.nonce, plaintext) {
            Ok(data) => Ok(data),
            Err(e) => bail!("use aes256gcm to encrypt data: {e:#}"),
        }
    }

    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        match self.cipher.decrypt(&self.nonce, ciphertext) {
            Ok(data) => Ok(data),
            Err(_) => bail!("decrypt failed, incorrect password or content"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};

    use super::*;

    #[test]
    fn test_cipher() {
        let path = "tests/aes.secret";
        let _ = fs::remove_file(path);

        let mut file = File::create(path).unwrap();

        let password = "some password 123";
        let cipher = AesCipher::new(password);
        cipher.write_head(&mut file).unwrap();

        let lines = [
            "hello, world!",
            "some longer text to encrypt, 1234567890!@#$%^&*()_+",
            "",
            "short",
        ];
        let plaintext = lines.join("\n");

        let text = cipher.encrypt(plaintext.as_bytes()).unwrap();
        file.write_all(text.as_slice()).unwrap();
        drop(file);

        let mut file = File::open(path).unwrap();
        let cipher = AesCipher::read(&file, password).unwrap();

        let mut ciphertext = Vec::new();
        file.read_to_end(&mut ciphertext).unwrap();

        let result = cipher.decrypt(&ciphertext).unwrap();
        let result = String::from_utf8(result).unwrap();

        assert_eq!(plaintext, result);
    }

    #[test]
    fn test_incorrect_password() {
        let path = "tests/aes_incorrect.secret";
        let _ = fs::remove_file(path);

        let mut file = File::create(path).unwrap();

        let password = "correct password";
        let cipher = AesCipher::new(password);
        cipher.write_head(&mut file).unwrap();
        let text = cipher.encrypt(b"some secret text").unwrap();
        file.write_all(&text).unwrap();
        drop(file);

        let mut file = File::open(path).unwrap();
        let cipher = AesCipher::read(&file, "incorrect password").unwrap();
        let mut ciphertext = Vec::new();
        file.read_to_end(&mut ciphertext).unwrap();
        assert_eq!(
            cipher.decrypt(&ciphertext).unwrap_err().to_string(),
            "decrypt failed, incorrect password or content"
        );
    }
}
