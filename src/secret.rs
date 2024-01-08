use std::fs::File;
use std::io;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::{FileExt, MetadataExt};
use std::path::Path;

use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, Nonce, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, KeyInit};
use anyhow::{bail, Context, Result};
use pbkdf2::pbkdf2_hmac_array;
use sha2::Sha256;

use crate::term;

const ENCRYPT_READ_BUFFER_SIZE: usize = 4096;
// const SHOW_PROGRESS_BAR_SIZE: u64 = 4096 * 1024;

const SALT_LENGTH: usize = 5;
const NONCE_LENGTH: usize = 12;

const SECRET_BEGIN_LINE: &'static str = "-----BEGIN ROXIDE SECRET-----";
const SECRET_END_LINE: &'static str = "-----END ROXIDE SECRET-----";

const PBKDF2_ROUNDS: u32 = 600_000;

pub fn handle<P: AsRef<Path>>(
    path: P,
    dest: &Option<String>,
    password: Option<&str>,
) -> Result<()> {
    let src = File::open(path.as_ref()).context("read file")?;
    let src_meta = src.metadata().context("get file meta")?;
    let dest: Box<dyn Write> = match dest.as_ref() {
        Some(dest) => {
            match File::open(dest) {
                Ok(dest_file) => {
                    let dest_meta = dest_file.metadata().context("get dest file meta")?;
                    if src_meta.ino() == dest_meta.ino() {
                        bail!("the dest file and src file can't be same");
                    }
                }
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => return Err(err).context("read dest file"),
            };

            let dest = File::create(dest).context("create dest file")?;
            Box::new(dest)
        }
        None => Box::new(io::stdout()),
    };

    let mut head_buffer: [u8; SECRET_BEGIN_LINE.len()] = [0; SECRET_BEGIN_LINE.len()];
    let read_count = src
        .read_at(&mut head_buffer, 0)
        .context("read head from src file")?;

    let password = password
        .map(|s| Ok(s.to_string()))
        .unwrap_or(term::input_password())?;
    if read_count == SECRET_BEGIN_LINE.len() {
        if let Ok(head) = String::from_utf8(head_buffer.to_vec()) {
            if head == SECRET_BEGIN_LINE {
                return decrypt(src, dest, password);
            }
        }
    }

    encrypt(src, dest, password)
}

fn encrypt<R, W, S>(plain: R, mut dest: W, password: S) -> Result<()>
where
    R: Read,
    W: Write,
    S: AsRef<str>,
{
    let mut reader = BufReader::new(plain);
    let mut buffer = [0; ENCRYPT_READ_BUFFER_SIZE];

    let mut write_data = |data: &[u8]| -> Result<()> {
        dest.write_all(data).context("write data to dest")?;
        dest.write(&[b'\n']).context("write break to dest")?;
        Ok(())
    };
    write_data(SECRET_BEGIN_LINE.as_bytes())?;

    let mut salt: [u8; SALT_LENGTH] = [0; SALT_LENGTH];
    let mut rng = OsRng::default();
    rng.fill_bytes(&mut salt);
    let salt_hex = hex::encode(&salt);
    write_data(&salt_hex.into_bytes())?;

    let key = pbkdf2_hmac_array::<Sha256, 32>(password.as_ref().as_bytes(), &salt, PBKDF2_ROUNDS);
    let key = Key::<Aes256Gcm>::from_slice(&key);

    let cipher = Aes256Gcm::new(&key);
    let nonce = Aes256Gcm::generate_nonce(&mut rng);
    assert_eq!(nonce.len(), NONCE_LENGTH);
    let nonce_hex = hex::encode(&nonce);
    write_data(&nonce_hex.into_bytes())?;

    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(bytes_read) => {
                let data = &buffer[..bytes_read];
                let encrypted = match cipher.encrypt(&nonce, data) {
                    Ok(data) => data,
                    Err(err) => bail!("use aes256gcm to encrypt data: {err}"),
                };
                let line = hex::encode(encrypted);
                write_data(&line.into_bytes())?;
            }
            Err(err) => return Err(err).context("read plain data"),
        }
    }
    write_data(SECRET_END_LINE.as_bytes())?;
    dest.flush().context("flush dest")?;

    Ok(())
}

fn decrypt<R, W, S>(encrypted: R, mut dest: W, password: S) -> Result<()>
where
    R: Read,
    W: Write,
    S: AsRef<str>,
{
    let reader = BufReader::new(encrypted);
    let mut lines = reader.lines();
    let mut must_read_line = || -> Result<String> {
        match lines.next() {
            Some(line) => line.context("read data from file"),
            None => bail!("unexpect end of the file, the file is too short"),
        }
    };

    let head = must_read_line()?;
    if head != SECRET_BEGIN_LINE {
        bail!("unexpect head line of the file");
    }

    let salt = hex::decode(must_read_line()?).context("decode salt as hex string")?;
    if salt.len() != SALT_LENGTH {
        bail!(
            "invalid salt length, expect {}, found {}",
            SALT_LENGTH,
            salt.len()
        );
    }
    let key = pbkdf2_hmac_array::<Sha256, 32>(password.as_ref().as_bytes(), &salt, PBKDF2_ROUNDS);
    let key = Key::<Aes256Gcm>::from_slice(&key);
    let cipher = Aes256Gcm::new(&key);

    let nonce = hex::decode(must_read_line()?).context("decode nonce as hex string")?;
    if nonce.len() != NONCE_LENGTH {
        bail!(
            "invalid nonce length, expect {}, found {}",
            NONCE_LENGTH,
            nonce.len()
        );
    }
    let nonce = Nonce::<Aes256Gcm>::from_slice(&nonce);

    for line in lines {
        let line = line.context("read content from file")?;
        if line == SECRET_END_LINE {
            break;
        }
        let buffer = hex::decode(line).context("decode content as hex string")?;
        let buffer: &[u8] = &buffer;
        let plain = match cipher.decrypt(&nonce, buffer) {
            Ok(data) => data,
            Err(_) => bail!("decrypt secret failed, incorrect password or content"),
        };
        dest.write_all(&plain).context("write buffer to dest")?;
    }
    dest.flush().context("flush dest")?;

    Ok(())
}
