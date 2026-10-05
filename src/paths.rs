use crate::error::{Result, VltError};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub fn vlt_home_from(vlt_home: Option<OsString>, home: Option<OsString>) -> Result<PathBuf> {
    if let Some(v) = vlt_home.filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(v));
    }
    let home = home
        .filter(|h| !h.is_empty())
        .ok_or_else(|| VltError::Internal("cannot locate the vlt home: $HOME is not set".into()))?;
    Ok(PathBuf::from(home).join(".vlt"))
}

pub fn vlt_home() -> Result<PathBuf> {
    vlt_home_from(std::env::var_os("VLT_HOME"), std::env::var_os("HOME"))
}

pub fn home_dir() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| VltError::Internal("$HOME is not set".into()))
}

pub fn config_path(vlt_home: &Path) -> PathBuf {
    vlt_home.join("config.toml")
}

pub fn index_dir(vlt_home: &Path) -> PathBuf {
    vlt_home.join("index")
}

pub fn models_dir(vlt_home: &Path) -> PathBuf {
    vlt_home.join("models")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vlt_home_prefers_override_then_home() {
        assert_eq!(vlt_home_from(Some("/x".into()), Some("/h".into())).unwrap(), PathBuf::from("/x"));
        assert_eq!(vlt_home_from(Some("".into()), Some("/h".into())).unwrap(), PathBuf::from("/h/.vlt"));
        assert_eq!(vlt_home_from(None, Some("/h".into())).unwrap(), PathBuf::from("/h/.vlt"));
        assert!(vlt_home_from(None, None).is_err());
    }
    #[test]
    fn layout() {
        let h = Path::new("/h/.vlt");
        assert_eq!(config_path(h), PathBuf::from("/h/.vlt/config.toml"));
        assert_eq!(index_dir(h), PathBuf::from("/h/.vlt/index"));
        assert_eq!(models_dir(h), PathBuf::from("/h/.vlt/models"));
    }
}
