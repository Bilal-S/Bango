//! Cross-platform detection of Chromium / Chrome installations.
//!
//! Preference order: Chromium → Google Chrome.

use std::path::PathBuf;
use std::process::Command;

/// Returned when no suitable browser is found on the system.
#[derive(Debug, thiserror::Error)]
pub enum BrowserError {
    #[error(
        "No Chromium or Google Chrome installation found.\n\
         Please install one of the following:\n\
         \n\
         Linux:   sudo apt install chromium-browser  OR  sudo dnf install chromium\n\
         macOS:   brew install --cask chromium       OR  brew install --cask google-chrome\n\
         Windows: Download from https://www.chromium.org/getting-involved/download-chromium\n\
         \n\
         Alternatively, install Google Chrome from https://www.google.com/chrome/"
    )]
    NotFound,
}

/// Platform-agnostic result of browser detection.
pub struct BrowserInfo {
    /// The executable path or command name to pass to `headless_chrome`.
    pub executable: PathBuf,
}

/// Try running `cmd --version` to see if the browser exists on PATH.
fn is_on_path(cmd: &str) -> Option<PathBuf> {
    Command::new(cmd)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|_| PathBuf::from(cmd))
}

/// Check if a file exists at the given path.
fn exists_at(path: &str) -> Option<PathBuf> {
    let pb = PathBuf::from(path);
    if pb.exists() {
        Some(pb)
    } else {
        None
    }
}

/// Detect a Chromium or Chrome executable on the current system.
///
/// Checks standard installation locations and the system `PATH`.
/// Prefers Chromium over Chrome.
pub fn detect_browser() -> Result<BrowserInfo, BrowserError> {
    // Platform-specific well-known paths, checked before PATH.
    let candidates: Vec<Option<PathBuf>> = if cfg!(target_os = "linux") {
        vec![
            is_on_path("chromium-browser"),
            is_on_path("chromium"),
            is_on_path("google-chrome-stable"),
            is_on_path("google-chrome"),
        ]
    } else if cfg!(target_os = "macos") {
        vec![
            exists_at("/Applications/Chromium.app/Contents/MacOS/Chromium"),
            exists_at("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
            is_on_path("chromium"),
            is_on_path("google-chrome"),
        ]
    } else if cfg!(target_os = "windows") {
        vec![
            exists_at(r"C:\Program Files\Chromium\Application\chrome.exe"),
            exists_at(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
            exists_at(r"C:\Program Files (x86)\Chromium\Application\chrome.exe"),
            exists_at(r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe"),
            {
                // Try `where` command on Windows
                Command::new("where")
                    .arg("chrome")
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .and_then(|o| {
                        String::from_utf8_lossy(&o.stdout)
                            .lines()
                            .next()
                            .map(|s| PathBuf::from(s.trim()))
                    })
            },
        ]
    } else {
        // Fallback for other platforms: just try PATH
        vec![
            is_on_path("chromium-browser"),
            is_on_path("chromium"),
            is_on_path("google-chrome-stable"),
            is_on_path("google-chrome"),
        ]
    };

    if let Some(executable) = candidates.into_iter().flatten().next() {
        Ok(BrowserInfo { executable })
    } else {
        Err(BrowserError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_browser_returns_result() {
        // On CI or development machines, a browser should be available.
        // This test just verifies the function doesn't panic and returns a
        // proper Result.
        let result = detect_browser();
        assert!(result.is_ok() || matches!(result, Err(BrowserError::NotFound)));
    }
}
