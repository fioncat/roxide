use std::io::{self, BufReader, Read, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

use crate::format::format_bytes;
use crate::secret::aes::AesCipher;
use crate::{cursor_up, outputln};

#[inline]
pub async fn encrypt<R, F, W>(
    password: &str,
    src: R,
    dest_factory: F,
    buffer_size: usize,
) -> Result<W>
where
    R: Read,
    F: Fn() -> Result<W>,
    W: Write,
{
    inner(password, src, dest_factory, buffer_size, true).await
}

#[inline]
pub async fn decrypt<R, F, W>(password: &str, src: R, dest_factory: F) -> Result<W>
where
    R: Read,
    F: Fn() -> Result<W>,
    W: Write,
{
    inner(password, src, dest_factory, 0, false).await
}

async fn inner<R, F, W>(
    password: &str,
    src: R,
    dest_factory: F,
    buffer_size: usize,
    encrypt: bool,
) -> Result<W>
where
    R: Read,
    F: Fn() -> Result<W>,
    W: Write,
{
    let mut reader = BufReader::new(src);
    let cipher = if encrypt {
        AesCipher::new(password)
    } else {
        AesCipher::read(&mut reader, password)?
    };
    let cipher = Arc::new(cipher);

    let worker_count = num_cpus::get();
    assert!(worker_count > 0);

    let mut task_tx_list: Vec<mpsc::Sender<(usize, Vec<u8>)>> = vec![];
    let (result_tx, mut result_rx) = mpsc::channel::<(usize, Result<Vec<u8>>)>(worker_count);

    let mut handlers = vec![];
    for _worker_idx in 0..worker_count {
        let cipher = cipher.clone();
        let (task_tx, mut task_rx) = mpsc::channel::<(usize, Vec<u8>)>(1);
        task_tx_list.push(task_tx);
        let result_tx = result_tx.clone();

        let handler = tokio::spawn(async move {
            loop {
                let Some((task_idx, data)) = task_rx.recv().await else {
                    break;
                };
                let result = if encrypt {
                    cipher.encrypt(&data)
                } else {
                    cipher.decrypt(&data)
                };
                result_tx.send((task_idx, result)).await.unwrap();
            }
        });
        handlers.push(handler);
    }

    let mut done = false;
    let mut dest: Option<W> = None;
    while !done {
        let mut sent = 0;
        for (idx, task_tx) in task_tx_list.iter().enumerate() {
            let data = if encrypt {
                let mut buffer = vec![0u8; buffer_size];
                match reader.read(&mut buffer)? {
                    0 => {
                        done = true;
                        break;
                    }
                    bytes_read => {
                        buffer.truncate(bytes_read);
                        buffer
                    }
                }
            } else {
                let mut len_buffer = [0u8; 4];
                let read_size = match reader.read_exact(&mut len_buffer) {
                    Ok(()) => u32::from_be_bytes(len_buffer) as usize,
                    Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                        done = true;
                        break;
                    }
                    Err(e) => bail!(e),
                };

                let mut data = vec![0u8; read_size];
                reader.read_exact(&mut data)?;
                data
            };

            task_tx.send((idx, data)).await.unwrap();
            sent += 1;
        }

        if sent == 0 {
            break;
        }

        let mut results: Vec<(usize, Vec<u8>)> = vec![];
        for _ in 0..sent {
            let (idx, result) = result_rx.recv().await.unwrap();
            let line = match result {
                Ok(line) => line,
                Err(e) => return Err(e),
            };
            results.push((idx, line));
        }
        results.sort_unstable_by(|(idx1, _), (idx2, _)| idx1.cmp(idx2));

        let w = match dest {
            Some(ref mut w) => w,
            None => {
                let mut w = dest_factory()?;
                if encrypt {
                    cipher.write_head(&mut w)?;
                }
                dest = Some(w);
                dest.as_mut().unwrap()
            }
        };

        for (_, data) in results {
            if encrypt {
                let data_len = data.len();
                if data_len > u32::MAX as usize {
                    bail!(
                        "data segment is too large: {data_len}, please use a smaller buffer size"
                    );
                }
                let data_len = data_len as u32;
                let len_bytes = data_len.to_be_bytes();
                w.write_all(&len_bytes)?;
            }
            w.write_all(&data)?;
        }
    }

    drop(task_tx_list);

    for handler in handlers {
        handler.await.unwrap();
    }

    match dest {
        Some(w) => Ok(w),
        None => bail!("the source file is empty, no data to handle"),
    }
}

pub struct Sha256Writer<W: Write> {
    inner: W,
    sha256: Sha256,
}

impl<W: Write> Sha256Writer<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            sha256: Sha256::new(),
        }
    }

    pub fn finalize(self) -> [u8; 32] {
        self.sha256.finalize().into()
    }
}

impl<W: Write> Write for Sha256Writer<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.sha256.update(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

pub struct ProgressReader<R: Read> {
    verb: &'static str,

    inner: R,

    bytes_read: u64,
    total: String,
    last_display: Instant,
    last_bytes: u64,
    displayed: bool,
    display_interval: Duration,
}

impl<R: Read> ProgressReader<R> {
    const DEFAULT_DISPLAY_INTERVAL: Duration = Duration::from_millis(100);

    pub fn new(verb: &'static str, inner: R, total: u64) -> Self {
        Self {
            verb,
            inner,
            bytes_read: 0,
            total: format_bytes(total),
            last_display: Instant::now(),
            last_bytes: 0,
            displayed: false,
            display_interval: Self::DEFAULT_DISPLAY_INTERVAL,
        }
    }

    fn display_progress(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.last_display) < self.display_interval {
            // No need to display
            return;
        }

        let elapsed = now.duration_since(self.last_display).as_secs_f64();
        let bytes_diff = self.bytes_read.saturating_sub(self.last_bytes);
        let speed = bytes_diff as f64 / elapsed;

        if self.displayed {
            cursor_up!();
        }
        outputln!(
            "{}: {} / {}, speed: {}/s",
            self.verb,
            format_bytes(self.bytes_read),
            self.total,
            format_bytes(speed as u64)
        );
        self.displayed = true;

        self.last_display = now;
        self.last_bytes = self.bytes_read;
    }
}

impl<R: Read> Read for ProgressReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.bytes_read += n as u64;
        self.display_progress();
        Ok(n)
    }
}

impl<R: Read> Drop for ProgressReader<R> {
    fn drop(&mut self) {
        if self.displayed {
            // Move cursor up to overwrite the last progress line
            cursor_up!();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};
    use std::io::Cursor;
    use std::path::Path;

    use rand::TryRngCore;
    use rand::rngs::OsRng;

    use super::*;

    fn generate_random_string(length: usize) -> String {
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

        let mut result = String::with_capacity(length);
        let mut rng = OsRng;

        for _ in 0..length {
            let idx = (rng.try_next_u32().unwrap() as usize) % CHARSET.len();
            result.push(CHARSET[idx] as char);
        }

        result
    }

    #[tokio::test]
    async fn test_encrypt_decrypt() {
        let size = 500000;
        let random_content = generate_random_string(size);

        let plain_path = "tests/random.txt";
        let secret_path = "tests/random.txt.secret";
        let result_path = "tests/random_result.txt";
        let incorrect_path = Path::new("tests/result_incorrect.txt");

        let _ = fs::remove_file(plain_path);
        let _ = fs::remove_file(secret_path);
        let _ = fs::remove_file(result_path);
        let _ = fs::remove_file(incorrect_path);

        fs::write(plain_path, &random_content).unwrap();

        let password = "test_password123";
        let buffer_size = 4096;
        let src = File::open(plain_path).unwrap();
        encrypt(
            password,
            src,
            || Ok(File::create(secret_path).unwrap()),
            buffer_size,
        )
        .await
        .unwrap();

        let src = File::open(secret_path).unwrap();
        decrypt(password, src, || Ok(File::create(result_path).unwrap()))
            .await
            .unwrap();

        let result_data = fs::read_to_string(result_path).unwrap();
        assert_eq!(random_content, result_data);

        let src = File::open(secret_path).unwrap();
        let result = decrypt("incorrect_password", src, || {
            Ok(File::create(incorrect_path).unwrap())
        })
        .await;
        assert!(result.is_err());
        // When password is incorrect, the destination file should not be created
        assert!(!incorrect_path.exists());
    }

    #[test]
    fn test_sha256_writer() {
        let size = 700000;
        let random_content = generate_random_string(size);

        let path = "tests/sha256_test.txt";

        let _ = fs::remove_file(path);
        let file = File::create(path).unwrap();
        let mut writer = Sha256Writer::new(file);
        writer.write_all(random_content.as_bytes()).unwrap();

        let hash = writer.finalize();
        let expect = Sha256::digest(random_content.as_bytes());
        assert_eq!(hash.as_slice(), expect.as_slice());

        let data = fs::read_to_string(path).unwrap();
        assert_eq!(data, random_content);
    }

    #[test]
    fn test_progress_reader() {
        let size = 300000;
        let random_content = generate_random_string(size);
        let cursor = Cursor::new(random_content.as_bytes());
        let mut reader = ProgressReader::new("Testing", cursor, size as u64);
        let mut buffer = vec![0u8; 8192];
        let mut total_read = 0;
        loop {
            let n = reader.read(&mut buffer).unwrap();
            if n == 0 {
                break;
            }
            total_read += n;
        }
        assert_eq!(total_read, size);
    }
}
