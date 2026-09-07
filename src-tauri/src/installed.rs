//! Which Undead Legacy build is actually sitting in the game folder.
//!
//! Directory presence tells us *a* mod is installed; it never tells us *which
//! build*, and the build is the only thing 7 Days to Die weighs when it decides
//! whether a client may join a server. With no recorded answer the launcher has
//! to redo the entire install — download, unpack and copy, several gigabytes —
//! to be certain of something it already did an hour ago.
//!
//! So an install writes down what it landed, and the next one reads it.

use crate::channel::Channel;
use crate::pin::PinnedPart;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Kept in the game folder, beside the files it describes. A marker stored
/// anywhere else survives someone deleting the mod by hand, and then lies.
const MARKER_FILE: &str = "ul_installed_build.json";

/// Bump only when the shape changes incompatibly. An unrecognised version reads
/// as "no idea what is installed", which costs a reinstall — never a bad join.
const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledBuild {
    pub schema_version: u32,
    pub channel: Channel,
    /// One build id per part, in the order they were unpacked.
    pub parts: Vec<String>,
    /// Seconds since the Unix epoch — for support questions, nothing decides on it.
    pub installed_at: u64,
}

impl InstalledBuild {
    /// Whether this marker describes exactly the build about to be installed.
    ///
    /// Order is part of the answer: parts unpack over one another, so the same
    /// ids in a different order describe a different tree.
    pub fn matches(&self, channel: Channel, parts: &[PinnedPart]) -> bool {
        self.schema_version == SCHEMA_VERSION
            && self.channel == channel
            && self.parts.len() == parts.len()
            && self
                .parts
                .iter()
                .zip(parts)
                .all(|(recorded, pinned)| recorded == pinned.build_id())
    }

    /// How the build reads in the UI and in support screenshots — the same
    /// "a + b" form the install result reports.
    pub fn describe(&self) -> String {
        self.parts.join(" + ")
    }
}

pub fn marker_path(game: &Path) -> PathBuf {
    game.join(MARKER_FILE)
}

/// The recorded build, or `None` when there is none we can trust.
///
/// Every failure — absent, unreadable, malformed, a schema from the future —
/// answers "unknown", which reinstalls. The opposite mistake, half-reading a
/// marker and believing it, is the one that strands a player at the join screen.
pub fn read(game: &Path) -> Option<InstalledBuild> {
    let text = fs::read_to_string(marker_path(game)).ok()?;
    let build: InstalledBuild = serde_json::from_str(&text).ok()?;
    (build.schema_version == SCHEMA_VERSION).then_some(build)
}

pub fn write(game: &Path, channel: Channel, parts: &[PinnedPart]) -> io::Result<()> {
    let build = InstalledBuild {
        schema_version: SCHEMA_VERSION,
        channel,
        parts: parts.iter().map(|p| p.build_id().to_string()).collect(),
        installed_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    };
    let text = serde_json::to_string_pretty(&build)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(marker_path(game), text)
}

/// Forget what is installed.
///
/// Called before the copy phase begins: from the moment the first folder is
/// replaced until the last one lands, the game holds a mix of two builds, and a
/// marker naming either of them is worse than no marker at all.
pub fn clear(game: &Path) {
    let _ = fs::remove_file(marker_path(game));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(sha: &str) -> PinnedPart {
        PinnedPart {
            label: "part".into(),
            group: "g".into(),
            repo: "r".into(),
            sha: sha.into(),
            mirror_sha256: None,
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ul-marker-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn round_trips_the_installed_build() {
        let game = scratch("round-trip");
        let parts = vec![
            part("4ea04e433d3b423fb3bfae326f7da7d14e95ab0b"),
            part("e890a4ead20da776a0554f7b098f2e613ebaccc1"),
        ];
        write(&game, Channel::Experimental, &parts).unwrap();

        let read_back = read(&game).expect("marker should be readable");
        assert!(read_back.matches(Channel::Experimental, &parts));
        assert_eq!(read_back.describe(), "4ea04e43 + e890a4ea");
        let _ = fs::remove_dir_all(&game);
    }

    /// The whole point: a pin bump must not read as "already installed", or the
    /// player keeps the old build and cannot join the server.
    #[test]
    fn a_pin_bump_does_not_match() {
        let game = scratch("bump");
        let before = vec![part("131fd4ea4e89fd0402082cd7e4851f1836091908")];
        let after = vec![part("4ea04e433d3b423fb3bfae326f7da7d14e95ab0b")];
        write(&game, Channel::Experimental, &before).unwrap();

        let recorded = read(&game).unwrap();
        assert!(recorded.matches(Channel::Experimental, &before));
        assert!(!recorded.matches(Channel::Experimental, &after));
        let _ = fs::remove_dir_all(&game);
    }

    #[test]
    fn another_channel_does_not_match() {
        let game = scratch("channel");
        let parts = vec![part("093b0380f76f3e5ad000d285f7136cbd63eabf3b")];
        write(&game, Channel::Stable, &parts).unwrap();

        let recorded = read(&game).unwrap();
        assert!(!recorded.matches(Channel::Experimental, &parts));
        let _ = fs::remove_dir_all(&game);
    }

    /// Two parts installed, one pinned: a prefix match is not a match.
    #[test]
    fn a_dropped_part_does_not_match() {
        let game = scratch("parts");
        let both = vec![
            part("4ea04e433d3b423fb3bfae326f7da7d14e95ab0b"),
            part("e890a4ead20da776a0554f7b098f2e613ebaccc1"),
        ];
        write(&game, Channel::Experimental, &both).unwrap();

        let recorded = read(&game).unwrap();
        assert!(!recorded.matches(Channel::Experimental, &both[..1]));
        let _ = fs::remove_dir_all(&game);
    }

    #[test]
    fn unreadable_markers_read_as_unknown() {
        let game = scratch("garbage");
        fs::write(marker_path(&game), b"not json").unwrap();
        assert!(read(&game).is_none());

        fs::write(
            marker_path(&game),
            br#"{"schemaVersion":99,"channel":"experimental","parts":["4ea04e43"],"installedAt":0}"#,
        )
        .unwrap();
        assert!(read(&game).is_none(), "a future schema must not be trusted");

        assert!(read(&scratch("empty")).is_none());
        let _ = fs::remove_dir_all(&game);
    }

    #[test]
    fn clearing_leaves_no_claim_behind() {
        let game = scratch("clear");
        let parts = vec![part("4ea04e433d3b423fb3bfae326f7da7d14e95ab0b")];
        write(&game, Channel::Experimental, &parts).unwrap();
        clear(&game);
        assert!(read(&game).is_none());
        let _ = fs::remove_dir_all(&game);
    }
}
