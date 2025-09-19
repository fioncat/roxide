mod aes;
mod core;
mod password;
mod scan;

use clap::Args;
use clap_num::number_range;

use std::fs::File;
use std::io::{self, Read, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::cmd::complete::CompleteArg;
use crate::config::context::ConfigContext;
use crate::info;

#[derive(Debug, Args)]
pub struct SecretArgs {
    /// The source file or directory to encrypt or decrypt. With `-f` option, it refers to
    /// a file; otherwise, it refers to a directory.
    /// In directory mode, it reads the directory's .gitignore file, finds patterns
    /// containing the `# rox:secret` marker, scans the directory for files matching those
    /// patterns, and encrypts or decrypts them. Encrypted files are renamed by adding a
    /// `.secret` suffix to the original name, while decryption removes the `.secret` suffix.
    /// If this option is not provided, directory mode defaults to the current directory;
    /// in file mode, data is read from stdin.
    pub src: Option<String>,

    /// Force update password, overwriting the existing password.
    #[arg(short)]
    pub update_password: bool,

    /// Indicates file mode. If a value is provided, the content is output to the target
    /// file; if no value is provided, the content is output to stdout.
    /// Note that binary content cannot be output to the terminal, such as when encrypting
    /// or decrypting non-UTF-8 content. When this option has no value, stdout must be
    /// redirected, otherwise an error will occur.
    /// If this option is provided with a value, the target file cannot be the same as the
    /// source file, i.e., the encryption and decryption files cannot be the same.
    #[arg(short)]
    pub file: Option<Option<String>>,
}

#[derive(Debug, Args)]
pub struct BufferArgs {
    /// The buffer size processed by each thread at once during concurrent encryption and
    /// decryption. Generally, larger values result in faster encryption and decryption
    /// speeds, but consume more memory. Cannot be lower than 512.
    #[arg(
        name = "buffer-size",
        long = "buffer-size",
        short = 'b',
        default_value = "4096",
        value_parser=buffer_size_range
    )]
    pub size: usize,
}

impl SecretArgs {
    pub fn get_password(&self, ctx: &ConfigContext) -> Result<String> {
        password::get_password(ctx, self.update_password)
    }

    pub fn into_many_base_dir(self, ctx: ConfigContext) -> Result<PathBuf> {
        let dir = match self.src {
            Some(ref s) => PathBuf::from(s),
            None => ctx.current_dir,
        };
        if dir.is_file() {
            bail!("the path you provided must be directory");
        }
        Ok(dir)
    }

    pub fn complete() -> [CompleteArg; 3] {
        [
            CompleteArg::new().files(),
            CompleteArg::new().short('u'),
            CompleteArg::new().short('f').files().array(),
        ]
    }
}

fn buffer_size_range(s: &str) -> Result<usize, String> {
    number_range(s, 512, usize::MAX)
}

const REPORT_PROGRESS_SIZE: u64 = 1024 * 1024 * 100; // 100 MiB

#[inline]
pub async fn encrypt_one<S, D>(
    src: Option<S>,
    dest: Option<D>,
    password: &str,
    buffer_size: usize,
) -> Result<()>
where
    S: AsRef<Path>,
    D: AsRef<Path>,
{
    one(src, dest, password, buffer_size, true, false).await?;
    Ok(())
}

#[inline]
pub async fn decrypt_one<S, D>(src: Option<S>, dest: Option<D>, password: &str) -> Result<()>
where
    S: AsRef<Path>,
    D: AsRef<Path>,
{
    one(src, dest, password, 0, false, false).await?;
    Ok(())
}

enum Input {
    File(File),
    Progress(core::ProgressReader<File>),
    Stdin(io::Stdin),
}

impl Read for Input {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Input::File(f) => f.read(buf),
            Input::Progress(p) => p.read(buf),
            Input::Stdin(s) => s.read(buf),
        }
    }
}

enum Output {
    File(File),
    Sha256(core::Sha256Writer<File>),
    StdoutWrap(core::StdoutWrap),
    Stdout(io::Stdout),
}

impl Write for Output {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Output::File(f) => f.write(buf),
            Output::Sha256(s) => s.write(buf),
            Output::StdoutWrap(w) => w.write(buf),
            Output::Stdout(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Output::File(f) => f.flush(),
            Output::Sha256(s) => s.flush(),
            Output::StdoutWrap(w) => w.flush(),
            Output::Stdout(s) => s.flush(),
        }
    }
}

async fn one<S, D>(
    src: Option<S>,
    dest: Option<D>,
    password: &str,
    buffer_size: usize,
    encrypt: bool,
    with_sha256: bool,
) -> Result<Option<String>>
where
    S: AsRef<Path>,
    D: AsRef<Path>,
{
    let mut src_metadata = None;
    let src: Input = match src {
        Some(path) => {
            let file = File::open(path.as_ref()).with_context(|| {
                format!("failed to open source file {:?}", path.as_ref().display())
            })?;
            let metadata = file.metadata()?;
            if metadata.len() > REPORT_PROGRESS_SIZE {
                let verb = if encrypt { "Encrypting" } else { "Decrypting" };
                let reader = core::ProgressReader::new(verb, file, metadata.len());
                src_metadata = Some(metadata);
                Input::Progress(reader)
            } else {
                Input::File(file)
            }
        }
        None => Input::Stdin(io::stdin()),
    };

    let dest_factory = || -> Result<Output> {
        match dest {
            Some(ref path) => {
                if let Some(ref src_metadata) = src_metadata
                    && path.as_ref().exists()
                {
                    let dest_metadata = path.as_ref().metadata().with_context(|| {
                        format!(
                            "failed to get metadata of destination file {:?}",
                            path.as_ref().display()
                        )
                    })?;
                    if src_metadata.ino() == dest_metadata.ino() {
                        // Source and destination files cannot be the same; otherwise,
                        // it may cause the program to hang (reading data and writing to
                        // the same file, resulting in an endless loop). Therefore, a
                        // precautionary measure needs to be taken here.
                        // TODO: The logic here uses the UNIX file system's INO for
                        // checking. If porting to Windows, the logic in this part
                        // needs to be modified.
                        bail!("the dest file and src file can't be same");
                    }
                }
                let file = File::create(path.as_ref()).with_context(|| {
                    format!(
                        "failed to create destination file {:?}",
                        path.as_ref().display()
                    )
                })?;
                if with_sha256 {
                    Ok(Output::Sha256(core::Sha256Writer::new(file)))
                } else {
                    Ok(Output::File(file))
                }
            }
            None => {
                let stdout = io::stdout();
                if termion::is_tty(&stdout) {
                    if encrypt {
                        bail!(
                            "cannot write encrypted data (no utf-8) to terminal, please redirect the stdout to a file"
                        );
                    }
                    Ok(Output::StdoutWrap(core::StdoutWrap { stdout }))
                } else {
                    Ok(Output::Stdout(stdout))
                }
            }
        }
    };

    let output = core::one(src, dest_factory, password, buffer_size, encrypt).await?;
    if let Output::Sha256(writer) = output {
        let sha256 = hex::encode(writer.finalize());
        return Ok(Some(sha256));
    }
    Ok(None)
}

#[inline]
pub async fn encrypt_many<P>(base_dir: P, password: &str, buffer_size: usize) -> Result<()>
where
    P: AsRef<Path>,
{
    many(base_dir.as_ref(), password, buffer_size, true).await
}

#[inline]
pub async fn decrypt_many<P>(base_dir: P, password: &str) -> Result<()>
where
    P: AsRef<Path>,
{
    many(base_dir.as_ref(), password, 0, false).await
}

async fn many<P>(base_dir: P, password: &str, buffer_size: usize, encrypt: bool) -> Result<()>
where
    P: AsRef<Path>,
{
    let files = scan::scan_secret_files(base_dir.as_ref()).await?;
    if files.is_empty() {
        bail!("no source or secret file to handle");
    }

    for file in files {
        let verb = if encrypt { "Encrypting" } else { "Decrypting" };
        info!("{verb}: {}", file.name);
        if encrypt {
            encrypt_one(
                Some(file.source_path),
                Some(file.secret_path),
                password,
                buffer_size,
            )
            .await?;
        } else {
            decrypt_one(Some(file.secret_path), Some(file.source_path), password).await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::repo::ensure_dir;

    use super::*;

    #[tokio::test]
    async fn test_one() {
        let password = "test_password123";

        let content = r#"
        Test Line
        Next Line
        Last Line
        Goodbye!
        "#;

        let src_path = "tests/secret_one.txt";
        let secret_path = "tests/secret_one.secret";
        let result_path = "tests/secret_one.result";

        fs::write(src_path, content).unwrap();

        encrypt_one(Some(src_path), Some(secret_path), password, 4096)
            .await
            .unwrap();
        decrypt_one(Some(secret_path), Some(result_path), password)
            .await
            .unwrap();

        let result = fs::read_to_string(result_path).unwrap();
        assert_eq!(content, result);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_many() {
        let gitignore_content = r#"
        # rox:secrets
        test1.txt
        test2.txt
        "#;

        let base_dir = PathBuf::from("tests/secret_many");
        let _ = fs::remove_dir_all(&base_dir);

        ensure_dir(&base_dir).unwrap();

        fs::write(base_dir.join(".gitignore"), gitignore_content).unwrap();

        let test1_content = "Content from test1";
        let test2_content = "Content from test2";

        fs::write(base_dir.join("test1.txt"), test1_content).unwrap();
        fs::write(base_dir.join("test2.txt"), test2_content).unwrap();

        let password = "test_secret_many123";
        encrypt_many(&base_dir, password, 4096).await.unwrap();

        assert!(base_dir.join("test1.txt.secret").exists());
        assert!(base_dir.join("test2.txt.secret").exists());

        fs::remove_file(base_dir.join("test1.txt")).unwrap();
        fs::remove_file(base_dir.join("test2.txt")).unwrap();

        decrypt_many(&base_dir, password).await.unwrap();

        let result1 = fs::read_to_string(base_dir.join("test1.txt")).unwrap();
        let result2 = fs::read_to_string(base_dir.join("test2.txt")).unwrap();

        assert_eq!(result1, test1_content);
        assert_eq!(result2, test2_content);
    }
}
