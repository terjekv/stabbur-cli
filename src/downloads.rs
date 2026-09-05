//! Verify bytes in a sibling temporary file before atomically publishing a destination.
use crate::{AppError, cli, output};
use serde::Serialize;
use sha2::{Digest, Sha256};
use stabbur_client::{Authenticated, Sha256Digest, blocking};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

#[derive(Serialize)]
struct DownloadReport {
    digest: String,
    output: String,
    size: u64,
    resumed: bool,
    verified: bool,
}

/// Private proof that a completed local transfer matches its requested identity and size.
pub(crate) struct VerifiedDownload {
    temporary: tempfile::NamedTempFile,
    size: u64,
    resumed: bool,
}
impl VerifiedDownload {
    pub(crate) fn fetch(
        client: &blocking::Client<Authenticated>,
        digest: &Sha256Digest,
        destination: &Path,
        resume: bool,
        overwrite: bool,
    ) -> Result<Self, AppError> {
        let metadata = client.artifacts().get(digest)?;
        let existing = match fs::symlink_metadata(destination) {
            Ok(value) => {
                if !value.is_file() {
                    return Err(AppError::DestinationExists);
                }
                Some(value)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        if existing.is_some() && !resume && !overwrite {
            return Err(AppError::DestinationExists);
        }
        if resume
            && existing
                .as_ref()
                .is_some_and(|existing| existing.len() > metadata.size)
        {
            return Err(AppError::DigestMismatch);
        }
        let parent = destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        let offset = if resume && existing.is_some() {
            std::io::copy(&mut File::open(destination)?, &mut temporary)?
        } else {
            0
        };
        if offset > metadata.size {
            return Err(AppError::DigestMismatch);
        }
        if offset < metadata.size {
            client.artifacts().download_to(
                digest,
                &mut temporary,
                (offset > 0).then_some(offset),
            )?;
        }
        temporary.flush()?;
        if temporary.as_file().metadata()?.len() != metadata.size {
            return Err(AppError::DigestMismatch);
        }
        temporary.seek(SeekFrom::Start(0))?;
        let mut hash = Sha256::new();
        let mut buffer = vec![0_u8; 64 * 1024];
        loop {
            let count = temporary.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
        if hex::encode(hash.finalize()) != digest.as_str() {
            return Err(AppError::DigestMismatch);
        }
        temporary.as_file().sync_all()?;
        Ok(Self {
            temporary,
            size: metadata.size,
            resumed: offset > 0,
        })
    }
    pub(crate) fn publish(self, destination: &Path, replace: bool) -> Result<(), AppError> {
        let result = if replace {
            self.temporary.persist(destination)
        } else {
            self.temporary.persist_noclobber(destination)
        };
        result.map_err(|error| AppError::Io(error.error))?;
        Ok(())
    }
}

pub(crate) fn download(
    client: &blocking::Client<Authenticated>,
    arguments: &cli::DownloadArgs,
    json_output: bool,
) -> Result<(), AppError> {
    let digest = arguments.digest.parse::<Sha256Digest>()?;
    let verified = VerifiedDownload::fetch(
        client,
        &digest,
        &arguments.output,
        arguments.resume,
        arguments.overwrite,
    )?;
    let report = DownloadReport {
        digest: digest.to_string(),
        output: arguments.output.display().to_string(),
        size: verified.size,
        resumed: verified.resumed,
        verified: true,
    };
    verified.publish(&arguments.output, arguments.resume || arguments.overwrite)?;
    if json_output {
        output::json(&report)
    } else {
        println!(
            "Downloaded {} bytes to {} (SHA-256 verified)",
            report.size, report.output
        );
        Ok(())
    }
}
