mod aes;
mod core;
mod password;

use std::fs::File;
use std::io::{self, Read, Write};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use anyhow::{Context, Result, bail};

pub use password::get_password;

const REPORT_PROGRESS_SIZE: u64 = 1024 * 1024 * 100; // 100 MiB

#[inline]
pub async fn encrypt_single<S, D>(
    src: Option<S>,
    dest: Option<D>,
    password: &str,
    buffer_size: usize,
) -> Result<()>
where
    S: AsRef<Path>,
    D: AsRef<Path>,
{
    single(src, dest, password, buffer_size, true, false).await?;
    Ok(())
}

#[inline]
pub async fn decrypt_single<S, D>(src: Option<S>, dest: Option<D>, password: &str) -> Result<()>
where
    S: AsRef<Path>,
    D: AsRef<Path>,
{
    single(src, dest, password, 0, false, false).await?;
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

async fn single<S, D>(
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

    let output = core::single(src, dest_factory, password, buffer_size, encrypt).await?;
    if let Output::Sha256(writer) = output {
        let sha256 = hex::encode(writer.finalize());
        return Ok(Some(sha256));
    }
    Ok(None)
}
