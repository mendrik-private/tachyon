//! Native portal chooser with an explicit starting directory. The pinned GPUI
//! PathPromptOptions does not expose the portal's current_folder option.
use std::path::{Path, PathBuf};

use ashpd::desktop::{
    ResponseError,
    file_chooser::{FileFilter, OpenFileRequest},
};

pub fn existing_directory(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|path| path.is_dir()).cloned()
}

fn local_path(uri: &str) -> Result<PathBuf, String> {
    url::Url::parse(uri)
        .map_err(|error| error.to_string())?
        .to_file_path()
        .map_err(|_| "The selected file is not a local file.".into())
}

pub async fn open_file(directory: Option<&Path>) -> Result<Option<PathBuf>, String> {
    let result = async {
        let markdown = FileFilter::new("Markdown files (*.md, *.markdown)")
            .glob("*.[mM][dD]")
            .glob("*.[mM][aA][rR][kK][dD][oO][wW][nN]");
        let request = OpenFileRequest::default()
            .title("Open a Markdown file")
            .accept_label("Open")
            .modal(true)
            .multiple(false)
            .directory(false)
            .filter(markdown.clone())
            .filter(FileFilter::new("All files").glob("*"))
            .current_filter(markdown)
            .current_folder::<&Path>(directory)?
            .send()
            .await?;
        request.response()
    }
    .await;
    match result {
        Ok(files) => files
            .uris()
            .first()
            .map(|uri| local_path(uri.as_str()))
            .transpose(),
        Err(ashpd::Error::Response(ResponseError::Cancelled)) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_urls_preserve_spaces_unicode_and_reject_remote_sources() {
        assert_eq!(
            local_path("file:///tmp/Field%20notes/%C3%A9.md").unwrap(),
            PathBuf::from("/tmp/Field notes/é.md")
        );
        assert!(local_path("https://example.com/file.md").is_err());
    }

    #[test]
    fn remembered_directory_has_priority_over_other_available_directories() {
        let remembered = std::env::temp_dir();
        let fallback = std::env::current_dir().expect("workspace exists");
        assert_ne!(remembered, fallback);
        assert_eq!(
            existing_directory(&[remembered.clone(), fallback]),
            Some(remembered)
        );
    }

    #[test]
    fn missing_remembered_directory_falls_back_without_panicking() {
        let available = std::env::temp_dir();
        let missing = available.join(format!("tachyon-missing-dialog-{}", std::process::id()));
        assert_eq!(
            existing_directory(&[missing, available.clone()]),
            Some(available)
        );
        assert_eq!(existing_directory(&[]), None);
    }
}
