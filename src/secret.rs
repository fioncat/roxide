use std::fs::File;
use std::io;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::{FileExt, MetadataExt};
use std::path::Path;

use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, Nonce, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, KeyInit};
use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::STANDARD as B64Engine;
use base64::Engine;
use pbkdf2::pbkdf2_hmac_array;
use sha2::Sha256;

use crate::progress::ProgressReader;
use crate::term;

const ENCRYPT_READ_BUFFER_SIZE: usize = 4096;
const SHOW_PROGRESS_BAR_SIZE: u64 = 4096 * 1024;

const SALT_LENGTH: usize = 5;
const NONCE_LENGTH: usize = 12;
const HEADER_LENGTH: usize = SALT_LENGTH + NONCE_LENGTH;

const SECRET_BEGIN_LINE: &str = "-----BEGIN ROXIDE SECRET-----";
const SECRET_END_LINE: &str = "-----END ROXIDE SECRET-----";

const PBKDF2_ROUNDS: u32 = 600_000;

struct StdoutWrap {
    stdout: io::Stdout,
}

impl Write for StdoutWrap {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match String::from_utf8(buf.to_vec()) {
            Ok(s) => s,
            Err(_) => return Err(io::Error::new(io::ErrorKind::Other, "the plain data is not utf-8 encoded (maybe a binary file), please consider save data to file by using `-f` flag or pipe")),
        };
        self.stdout.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stdout.flush()
    }
}

/// Encrypts or decrypts files using the AES-256-GCM algorithm. A password is
/// required to represent the encryption key.
///
/// See:
///
/// * [AES](https://en.wikipedia.org/wiki/Advanced_Encryption_Standard)
/// * [GCM](https://en.wikipedia.org/wiki/Galois/Counter_Mode)
///
/// ## Encryption
///
/// 1. Generates a random `salt` of length 5.
/// 2. Utilizes the PBKDF2 + SHA256 algorithm to derive a key of length 32 bytes
///    from the user-provided password. The key undergoes salting and PBKDF2 processing
///    to enhance security.
/// 3. Generates a random `nonce` of length 12 for use in AES-256-GCM encryption.
/// 4. Reads 4096 bytes of data from the file at a time, encrypts using AES-256-GCM,
///    and stores the Base64 result as a string.
///
/// See:
///
/// * [PBKDF2](https://en.wikipedia.org/wiki/PBKDF2)
/// * [SHA-2](https://en.wikipedia.org/wiki/SHA-2)
///
/// The format of the encrypted file:
///
/// - First line: File begin, indicating that the file is an encrypted file using
///   the roxide encryption.
/// - Second line: Secret header, randomly generated salt + nonce.
/// - Subsequent lines: Each line represents the encrypted result of 4096 bytes of
///   original data.
/// - Last line: File footer, marking the end of the encrypted content; subsequent
///   lines after this are ignored.
///
/// In summary, the encrypted file is a text file, easily viewable and storable.
/// However, the encrypted file is generally larger than the original file and users
/// may need to compress it manually.
///
/// ## Decryption
///
/// In essence, it is the reverse process of encryption. The processing function
/// will determine whether the file is a roxide encrypted file based on the first
/// line of the file. If it is, the function will parse the salt and nonce from the
/// first few lines of the file. Then, it decrypts the data line by line, decrypting
/// 4096 bytes of original data each time (i.e., decrypting one line of encrypted
/// content from the encrypted file).
///
/// ## Arguments
///
/// * `path` - The source file. The function does not need to be informed whether
///   the file is encrypted or decrypted; it will determine it based on the file header.
///   If the file is encrypted, the function will decrypt the file content; otherwise,
///   it will encrypt the file content.
/// * `dest` - If the value is [`None`], the encryption/decryption result will
///   be directly output to stdout. If it is [`Some`], the result will be output
///   to a file.
/// * `password` - The password entered by the user. If the password is incorrect,
///   decryption will fail. If [`None`], the function will prompt the user to enter a
///   password.
pub fn handle<P: AsRef<Path>>(
    path: P,
    dest: &Option<String>,
    password: Option<&str>,
) -> Result<()> {
    let src = File::open(path.as_ref()).context("read file")?;
    let src_meta = src.metadata().context("get file meta")?;
    let mut is_dest_file = false;
    let dest: Box<dyn Write> = match dest.as_ref() {
        Some(dest) => {
            match File::open(dest) {
                Ok(dest_file) => {
                    // Source and destination files cannot be the same; otherwise,
                    // it may cause the program to hang (reading data and writing to
                    // the same file, resulting in an endless loop). Therefore, a
                    // precautionary measure needs to be taken here.
                    // TODO: The logic here uses the UNIX file system's INO for
                    // checking. If porting to Windows, the logic in this part
                    // needs to be modified.
                    let dest_meta = dest_file.metadata().context("get dest file meta")?;
                    if src_meta.ino() == dest_meta.ino() {
                        bail!("the dest file and src file can't be same");
                    }
                }
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => return Err(err).context("read dest file"),
            };

            is_dest_file = true;
            let dest = File::create(dest).context("create dest file")?;
            Box::new(dest)
        }
        None => {
            let stdout = io::stdout();
            if termion::is_tty(&stdout) {
                // If the result is to be output to the terminal, it must be ensured
                // that the result is in UTF-8 format to prevent confusion.
                // Therefore, [`StdoutWrap`] is used to validate it in real-time.
                Box::new(StdoutWrap { stdout })
            } else {
                is_dest_file = true;
                Box::new(stdout)
            }
        }
    };

    let mut head_buffer: [u8; SECRET_BEGIN_LINE.len()] = [0; SECRET_BEGIN_LINE.len()];
    let read_count = src
        .read_at(&mut head_buffer, 0)
        .context("read head from src file")?;
    let mut is_encrypt = true;
    if read_count == SECRET_BEGIN_LINE.len() {
        if let Ok(head) = String::from_utf8(head_buffer.to_vec()) {
            if head == SECRET_BEGIN_LINE {
                is_encrypt = false;
            }
        }
    }
    let password = password
        .map(|s| Ok(s.to_string()))
        .unwrap_or(term::input_password(is_encrypt))?;

    let reader: Box<dyn Read> = if is_dest_file && src_meta.len() > SHOW_PROGRESS_BAR_SIZE {
        // The progress bar for encryption/decryption will only be displayed in the
        // terminal if writing to a file and the source file is large enough.
        Box::new(ProgressReader::new(
            "Processing".to_string(),
            "Process".to_string(),
            src_meta.len() as usize,
            src,
        ))
    } else {
        Box::new(src)
    };

    if is_encrypt {
        encrypt(reader, dest, password).context("encrypt file")
    } else {
        decrypt(reader, dest, password).context("decrypt file")
    }
}

/// See: [`handle`].
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
        dest.write(b"\n").context("write break to dest")?;
        dest.flush().context("flush dest")?;
        Ok(())
    };
    write_data(SECRET_BEGIN_LINE.as_bytes())?;

    // Generate salt.
    let mut salt: [u8; SALT_LENGTH] = [0; SALT_LENGTH];
    let mut rng = OsRng;
    rng.fill_bytes(&mut salt);

    // Use PBKDF2 to generate private key according to user password and generated
    // salt. This approach ensures that the generated key is robust enough, and the
    // original password is less likely to be easily exposed.
    let key = pbkdf2_hmac_array::<Sha256, 32>(password.as_ref().as_bytes(), &salt, PBKDF2_ROUNDS);
    let key = Key::<Aes256Gcm>::from_slice(&key);

    let cipher = Aes256Gcm::new(key);
    // Generate the nonce in aes-256-gcm.
    let nonce = Aes256Gcm::generate_nonce(&mut rng);
    assert_eq!(nonce.len(), NONCE_LENGTH);

    // Write salt and nonce into file header.
    let mut head = salt.to_vec();
    head.extend(nonce.to_vec());
    let head_b64 = B64Engine.encode(head);
    write_data(&head_b64.into_bytes())?;

    loop {
        // Encrypts 4096 bytes of data from the source file at a time and writes it
        // as one line to the destination file.
        // The reason for encrypting in batches is to prevent the program from
        // consuming excessive memory by loading the entire source data into memory,
        // especially when the source file is large.
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(bytes_read) => {
                let data = &buffer[..bytes_read];
                let encrypted = match cipher.encrypt(&nonce, data) {
                    Ok(data) => data,
                    Err(err) => bail!("use aes256gcm to encrypt data: {err}"),
                };
                let line = B64Engine.encode(encrypted);
                write_data(&line.into_bytes())?;
            }
            Err(err) => return Err(err).context("read plain data"),
        }
    }
    write_data(SECRET_END_LINE.as_bytes())?;

    Ok(())
}

/// See: [`handle`].
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
            None => bail!("unexpected end of the file, the file is too short"),
        }
    };

    let begin = must_read_line()?;
    if begin != SECRET_BEGIN_LINE {
        bail!("unexpected begin line of the file");
    }

    let head = B64Engine
        .decode(must_read_line()?)
        .context("decode header as base64 string")?;
    if head.len() != HEADER_LENGTH {
        bail!(
            "invalid header length, expect {}, found {}",
            HEADER_LENGTH,
            head.len()
        );
    }

    let salt = &head[..SALT_LENGTH];
    let nonce = &head[SALT_LENGTH..];

    let key = pbkdf2_hmac_array::<Sha256, 32>(password.as_ref().as_bytes(), salt, PBKDF2_ROUNDS);
    let key = Key::<Aes256Gcm>::from_slice(&key);
    let cipher = Aes256Gcm::new(key);

    let nonce = Nonce::<Aes256Gcm>::from_slice(nonce);

    for line in lines {
        // During decryption, each line represents a batch, and each batch of data
        // is decrypted one at a time.
        let line = line.context("read content from file")?;
        if line == SECRET_END_LINE {
            break;
        }
        let buffer = B64Engine
            .decode(line)
            .context("decode content as base64 string")?;
        let buffer: &[u8] = &buffer;
        let plain = match cipher.decrypt(nonce, buffer) {
            Ok(data) => data,
            Err(_) => bail!("decrypt secret failed, incorrect password or content"),
        };
        dest.write_all(&plain).context("write buffer to dest")?;
        dest.flush().context("flush dest")?;
    }

    Ok(())
}
