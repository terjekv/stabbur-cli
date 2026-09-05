use std::{fs, io::Write, path::PathBuf};

use serde::{Deserialize, Serialize};
use stabbur_client::SecretToken;

use crate::AppError;

#[derive(Serialize, Deserialize)]
pub struct Profile {
    pub server: String,
    pub token: String,
}

pub fn path(explicit: Option<PathBuf>) -> Result<PathBuf, AppError> {
    if let Some(path) = explicit {
        return Ok(path);
    }
    let root = dirs::config_dir().ok_or(AppError::NoProfileDirectory)?;
    Ok(root.join("stabbur/profile.json"))
}

pub fn load(path: &std::path::Path) -> Result<Option<Profile>, AppError> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(AppError::Io(error)),
    };
    if !metadata.is_file() {
        return Err(AppError::UnsafeCredentialFile);
    }
    enforce_owner_only(&metadata)?;
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| AppError::InvalidProfile)
}

pub fn save(path: &std::path::Path, profile: &Profile) -> Result<(), AppError> {
    let parent = path.parent().ok_or(AppError::NoProfileDirectory)?;
    fs::create_dir_all(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    serde_json::to_writer(&mut temporary, profile).map_err(|_| AppError::InvalidProfile)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| AppError::Io(error.error))?;
    Ok(())
}

pub fn token_from_file(path: &std::path::Path) -> Result<SecretToken, AppError> {
    SecretToken::new(secret_from_file(path)?).map_err(AppError::Client)
}

pub fn secret_from_file(path: &std::path::Path) -> Result<String, AppError> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(AppError::UnsafeCredentialFile);
    }
    enforce_owner_only(&metadata)?;
    let value = fs::read_to_string(path)?;
    Ok(value.trim_end_matches(['\r', '\n']).to_owned())
}

pub fn write_secret_file(path: &std::path::Path, secret: &str) -> Result<(), AppError> {
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(secret.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn enforce_owner_only(metadata: &fs::Metadata) -> Result<(), AppError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(AppError::UnsafeCredentialFile);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_outputs_are_exclusive_and_round_trip() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("credential");
        write_secret_file(&path, "one-time-secret").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "one-time-secret\n");
        assert!(write_secret_file(&path, "replacement").is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "one-time-secret\n");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(path).unwrap().permissions().mode() & 0o077, 0);
        }
    }

    #[cfg(unix)]
    #[test]
    fn group_readable_credentials_are_rejected() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("credential");
        fs::write(&path, "unsafe-secret\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        assert!(token_from_file(&path).is_err());
    }

    #[test]
    fn protected_password_files_remove_only_line_endings() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("password");
        write_secret_file(&path, "  intentional padding  \r").unwrap();
        assert_eq!(secret_from_file(&path).unwrap(), "  intentional padding  ");
    }
}
