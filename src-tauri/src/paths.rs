//! Path helpers that always expand `~` — a major Mac bug in ModLauncher V5.

use std::path::PathBuf;

/// Expand a user path safely.
///
/// Never leave a leading `~` unexpanded. V5 saved `~/Library/...` literally and
/// created a fake folder named `~` under its Application Support directory.
pub fn expand_user_path(input: &str) -> PathBuf {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return PathBuf::new();
    }

    // Already absolute and not tilde-based.
    if trimmed.starts_with('/') {
        return PathBuf::from(trimmed);
    }

    // `~` or `~/...`
    if trimmed == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }

    // `$HOME/...` style (seen in some launcher path lists)
    if let Some(rest) = trimmed.strip_prefix("$HOME/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    if let Some(rest) = trimmed.strip_prefix("%HOME%/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }

    PathBuf::from(trimmed)
}

pub fn default_steam_game_path() -> PathBuf {
    expand_user_path("~/Library/Application Support/Steam/steamapps/common/7 Days To Die")
}

pub fn default_steam_manifest_path() -> PathBuf {
    expand_user_path("~/Library/Application Support/Steam/steamapps/appmanifest_251570.acf")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_tilde() {
        let p = expand_user_path("~/Library/Application Support/Steam");
        assert!(p.is_absolute());
        assert!(!p.to_string_lossy().starts_with('~'));
        assert!(p.to_string_lossy().contains("Library/Application Support/Steam"));
    }

    #[test]
    fn absolute_unchanged() {
        let p = expand_user_path("/Users/test/game");
        assert_eq!(p, PathBuf::from("/Users/test/game"));
    }
}
