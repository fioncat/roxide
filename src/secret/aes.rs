use std::io::{BufReader, Lines, Read, Write};

use aes_gcm::aead::{Aead, Nonce, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, KeyInit};
use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64Engine;
use pbkdf2::pbkdf2_hmac_array;
use rand::RngCore;
use sha2::Sha256;

pub struct AesCipher {
    cipher: Aes256Gcm,
    nonce: Nonce<Aes256Gcm>,
}

impl AesCipher {
    const SALT_LENGTH: usize = 20;
    const NONCE_LENGTH: usize = 12;
    #[cfg(not(test))]
    const PBKDF2_ROUNDS: u32 = 600_000;
    #[cfg(test)]
    const PBKDF2_ROUNDS: u32 = 600;

    pub fn generate<W>(mut dest: W, password: &str) -> Result<Self>
    where
        W: Write,
    {
        let mut rng = rand::rng();
        let mut salt = [0u8; Self::SALT_LENGTH];
        rng.fill_bytes(&mut salt);

        let salt_base64 = B64Engine.encode(salt);
        dest.write_all(salt_base64.as_bytes())
            .context("write salt to dest")?;
        dest.write_all(b"\n")?;

        let mut rng = OsRng;
        let nonce = Aes256Gcm::generate_nonce(&mut rng);

        let nonce_base64 = B64Engine.encode(nonce);
        dest.write_all(nonce_base64.as_bytes())
            .context("write nonce to dest")?;
        dest.write_all(b"\n")?;

        let key: [u8; 32] =
            pbkdf2_hmac_array::<Sha256, 32>(password.as_bytes(), &salt, Self::PBKDF2_ROUNDS);
        let key = Key::<Aes256Gcm>::from_slice(&key);
        let cipher = Aes256Gcm::new(key);

        Ok(Self { cipher, nonce })
    }

    pub fn read<R>(lines: &mut Lines<BufReader<R>>, password: &str) -> Result<Self>
    where
        R: Read,
    {
        let salt = Self::read_head(lines).context("read salt from encrypted data")?;
        if salt.len() != Self::SALT_LENGTH {
            bail!("the length of salt is incorrect");
        }

        let nonce = Self::read_head(lines).context("read nonce from encrypted data")?;
        if nonce.len() != Self::NONCE_LENGTH {
            bail!("the length of nonce is incorrect");
        }

        let key: [u8; 32] =
            pbkdf2_hmac_array::<Sha256, 32>(password.as_bytes(), &salt, Self::PBKDF2_ROUNDS);
        let key = Key::<Aes256Gcm>::from_slice(&key);
        let cipher = Aes256Gcm::new(key);

        let nonce = Nonce::<Aes256Gcm>::from_slice(&nonce);

        Ok(Self {
            cipher,
            nonce: *nonce,
        })
    }

    fn read_head<R>(lines: &mut Lines<BufReader<R>>) -> Result<Vec<u8>>
    where
        R: Read,
    {
        let Some(line) = lines.next() else {
            bail!("unexpected end of the encrypted data, the content is too short");
        };
        let line = line.context("read head line from encrypted data")?;
        let data = B64Engine.decode(line).context("decode base64 head line")?;
        Ok(data)
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Result<String> {
        let data = match self.cipher.encrypt(&self.nonce, plaintext) {
            Ok(data) => data,
            Err(e) => bail!("use aes256gcm to encrypt data: {e:#}"),
        };
        Ok(B64Engine.encode(data))
    }

    pub fn decrypt(&self, ciphertext: &str) -> Result<Vec<u8>> {
        let ciphertext = B64Engine
            .decode(ciphertext)
            .context("decode base64 ciphertext")?;
        let ciphertext: &[u8] = &ciphertext;
        match self.cipher.decrypt(&self.nonce, ciphertext) {
            Ok(data) => Ok(data),
            Err(_) => bail!("decrypt failed, incorrect password or content"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};
    use std::io::BufRead;

    use super::*;

    #[test]
    fn test_cipher() {
        let path = "tests/aes.secret";
        let _ = fs::remove_file(path);

        let mut file = File::create(path).unwrap();

        let password = "some password 123";
        let cipher = AesCipher::generate(&mut file, password).unwrap();

        let plaintexts = [
            "hello, world!",
            "some longer text to encrypt, 1234567890!@#$%^&*()_+",
            "",
            "short",
        ];

        for plaintext in plaintexts.iter() {
            let line = cipher.encrypt(plaintext.as_bytes()).unwrap();
            file.write_all(line.as_bytes()).unwrap();
            file.write_all(b"\n").unwrap();
        }
        drop(file);

        let file = File::open(path).unwrap();
        let mut lines = BufReader::new(file).lines();

        let cipher = AesCipher::read(&mut lines, password).unwrap();

        for (i, line) in lines.enumerate() {
            let line = line.unwrap();
            let result = cipher.decrypt(&line).unwrap();
            let result = String::from_utf8(result).unwrap();
            assert_eq!(result, plaintexts[i]);
        }
    }

    #[test]
    fn test_incorrect_password() {
        let path = "tests/aes_incorrect.secret";
        let _ = fs::remove_file(path);

        let mut file = File::create(path).unwrap();

        let password = "correct password";
        let cipher = AesCipher::generate(&mut file, password).unwrap();
        let line = cipher.encrypt(b"some secret text").unwrap();
        file.write_all(line.as_bytes()).unwrap();
        file.write_all(b"\n").unwrap();
        drop(file);

        let file = File::open(path).unwrap();
        let mut lines = BufReader::new(file).lines();
        let cipher = AesCipher::read(&mut lines, "incorrect password").unwrap();
        let line = lines.next().unwrap().unwrap();
        assert_eq!(
            cipher.decrypt(&line).unwrap_err().to_string(),
            "decrypt failed, incorrect password or content"
        );
    }
}
