use serde::{Deserialize, Serialize};

/// `[uploads]`: where files dropped or pasted on an SSH tab are written.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "kebab-case")]
pub struct Uploads {
    /// Directory on POSIX hosts; empty means `/tmp`.
    pub dir: String,
    /// Directory on Windows hosts; empty means the host's `%TEMP%`.
    pub windows_dir: String,
}

#[cfg(test)]
mod tests {
    use super::Uploads;

    #[derive(serde::Deserialize)]
    struct Document {
        uploads: Uploads,
    }

    fn parse(src: &str) -> Uploads {
        toml::from_str::<Document>(src).unwrap().uploads
    }

    #[test]
    fn upload_directories_are_configurable_and_default_to_empty() {
        let uploads = parse("[uploads]\ndir = \"/var/tmp\"\nwindows-dir = 'D:\\Drop'\n");
        assert_eq!(uploads.dir, "/var/tmp");
        assert_eq!(uploads.windows_dir, r"D:\Drop");
        let uploads = parse("[uploads]\ndir = \"/srv\"\n");
        assert_eq!(uploads.dir, "/srv");
        assert!(
            uploads.windows_dir.is_empty(),
            "unset keys keep their default"
        );
        assert_eq!(Uploads::default(), parse("[uploads]\n"));
    }
}
