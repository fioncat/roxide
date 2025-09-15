use std::io::Write;

use aes_gcm::aead::{Aead, Nonce, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, KeyInit};
use anyhow::Result;
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
    #[cfg(not(test))]
    const PBKDF2_ROUNDS: u32 = 600_000;
    #[cfg(test)]
    const PBKDF2_ROUNDS: u32 = 600;

    pub fn generate<W>(dest: W, password: &str) -> Result<Self>
    where
        W: Write,
    {
        let mut rng = rand::rng();
        let mut salt = [0u8; Self::SALT_LENGTH];
        rng.fill_bytes(&mut salt);

        let mut rng = OsRng;
        let nonce = Aes256Gcm::generate_nonce(&mut rng);

        let key: [u8; 32] =
            pbkdf2_hmac_array::<Sha256, 32>(password.as_bytes(), &salt, Self::PBKDF2_ROUNDS);

        todo!()
    }
}
