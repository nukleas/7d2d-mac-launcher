//! Which Undead Legacy build to install, resolved at run time.
//!
//! 7 Days to Die refuses connections between a client and server on different
//! mod builds, and the 2.7 line publishes every few days. Compiling the pinned
//! commits into the app therefore means reissuing a signed, notarized build —
//! and asking everyone to reinstall — every time the mod moves.
//!
//! Instead the pins live in a small JSON file fetched at install time. Bumping
//! the mod becomes editing one file, and every client follows on its next
//! install. The server reads the same file, so the two cannot drift apart —
//! which is the failure that actually strands people.
//!
//! The compiled-in values remain the fallback: if the manifest is unreachable,
//! malformed, or describes a channel we don't know, the install proceeds on
//! what shipped in the binary. A friend offline or behind a captive portal gets
//! a working install, never a failure.

use crate::channel::Channel;
use serde::Deserialize;
use std::time::Duration;

/// Where the pins are published. Raw file on the default branch, so updating it
/// is a one-line commit rather than a release.
const MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/nukleas/7d2d-mac-launcher/main/pinned-build.json";

/// The manifest is tiny; anything slower than this is a captive portal or a
/// dead link, and we would rather install from the built-in pins than hang.
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// Only bump when the shape changes incompatibly. An unknown version is
/// ignored rather than guessed at.
const SUPPORTED_SCHEMA: u32 = 1;

/// One archive making up a channel's package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedPart {
    pub label: String,
    pub group: String,
    pub repo: String,
    /// Exact commit to install.
    pub sha: String,
    /// SHA-256 of the mirror's copy, when one is published. Absent means the
    /// mirror is not offered for this part — GitLab's archives are generated
    /// per request and not byte-reproducible, so no digest can describe them.
    pub mirror_sha256: Option<String>,
}

impl PinnedPart {
    pub fn url(&self) -> String {
        format!(
            "https://gitlab.com/{}/{}/-/archive/{}/{}-{}.zip",
            self.group, self.repo, self.sha, self.repo, self.sha
        )
    }

    /// Short form for logs and the UI, so a player can compare against the
    /// server at a glance.
    pub fn build_id(&self) -> &str {
        let end = self.sha.len().min(8);
        &self.sha[..end]
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    schema_version: u32,
    #[serde(default)]
    channels: ManifestChannels,
}

#[derive(Debug, Default, Deserialize)]
struct ManifestChannels {
    #[serde(default)]
    stable: Option<ManifestChannel>,
    #[serde(default)]
    experimental: Option<ManifestChannel>,
}

#[derive(Debug, Deserialize)]
struct ManifestChannel {
    parts: Vec<ManifestPart>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestPart {
    label: String,
    group: String,
    repo: String,
    sha: String,
    #[serde(default)]
    mirror_sha256: Option<String>,
}

/// A commit is 40 hex characters; a digest is 64. Anything else is a typo or a
/// truncated paste, and installing from it would waste gigabytes before
/// failing — so reject the manifest and use the built-in pins.
fn is_hex(value: &str, len: usize) -> bool {
    value.len() == len && value.chars().all(|c| c.is_ascii_hexdigit())
}

impl ManifestPart {
    fn validate(self) -> Option<PinnedPart> {
        if !is_hex(&self.sha, 40) {
            return None;
        }
        if self.group.is_empty() || self.repo.is_empty() {
            return None;
        }
        // A malformed digest would fail every verification; drop it and fall
        // back to trusting the commit SHA, rather than blocking the install.
        let mirror_sha256 = self.mirror_sha256.filter(|d| is_hex(d, 64));
        Some(PinnedPart {
            label: self.label,
            group: self.group,
            repo: self.repo,
            sha: self.sha,
            mirror_sha256,
        })
    }
}

/// Parse and validate a manifest for one channel.
///
/// Returns `None` — meaning "use the built-in pins" — for anything we don't
/// fully understand. Partial trust is worse than no trust here: half a package
/// from one build and half from another produces an install that looks fine.
pub fn parse(text: &str, channel: Channel) -> Option<Vec<PinnedPart>> {
    let manifest: Manifest = serde_json::from_str(text).ok()?;
    if manifest.schema_version != SUPPORTED_SCHEMA {
        return None;
    }
    let entry = match channel {
        Channel::Stable => manifest.channels.stable,
        Channel::Experimental => manifest.channels.experimental,
    }?;
    if entry.parts.is_empty() {
        return None;
    }
    let declared = entry.parts.len();
    let resolved: Vec<PinnedPart> = entry
        .parts
        .into_iter()
        .filter_map(|p| p.validate())
        .collect();
    // All or nothing. Dropping just the invalid parts would assemble a package
    // from some manifest entries and some built-in ones — two different builds
    // merged into one install that looks perfectly fine.
    (resolved.len() == declared).then_some(resolved)
}

/// Fetch the published pins, falling back to the compiled-in ones.
///
/// Never fails: an unreachable or unparseable manifest yields the built-ins.
pub fn resolve(channel: Channel) -> (Vec<PinnedPart>, PinSource) {
    match fetch_manifest() {
        Some(text) => match parse(&text, channel) {
            Some(parts) => (parts, PinSource::Published),
            None => (channel.built_in_pins(), PinSource::BuiltInManifestRejected),
        },
        None => (channel.built_in_pins(), PinSource::BuiltInOffline),
    }
}

/// Where the pins in use came from — surfaced in the install log so a mismatch
/// with the server can be diagnosed without guessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinSource {
    Published,
    BuiltInOffline,
    BuiltInManifestRejected,
}

impl PinSource {
    pub fn describe(self) -> &'static str {
        match self {
            PinSource::Published => "published build list",
            PinSource::BuiltInOffline => "built-in build list (couldn't reach the published one)",
            PinSource::BuiltInManifestRejected => {
                "built-in build list (the published one looked wrong)"
            }
        }
    }
}

fn fetch_manifest() -> Option<String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("7d2d-mac-launcher (pin manifest)")
        .timeout(FETCH_TIMEOUT)
        .build()
        .ok()?;
    let resp = client.get(MANIFEST_URL).send().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.text().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"{
      "schemaVersion": 1,
      "channels": {
        "experimental": {
          "parts": [
            {"label":"part 1 of 2","group":"subquakesgroup","repo":"UndeadLegacyExperimentalPart1",
             "sha":"131fd4ea4e89fd0402082cd7e4851f1836091908",
             "mirrorSha256":"34021b9555e42d97cb9d1383b09fa437821caba9dc848bfe446be96150fb01c0"},
            {"label":"part 2 of 2","group":"subquakesgroup","repo":"UndeadLegacyExperimentalPart2",
             "sha":"e890a4ead20da776a0554f7b098f2e613ebaccc1"}
          ]
        }
      }
    }"#;

    #[test]
    fn parses_a_published_manifest() {
        let parts = parse(GOOD, Channel::Experimental).expect("should parse");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].build_id(), "131fd4ea");
        assert!(parts[0]
            .url()
            .contains("131fd4ea4e89fd0402082cd7e4851f1836091908"));
        assert!(parts[0].mirror_sha256.is_some());
        // A part with no digest simply isn't mirrored.
        assert!(parts[1].mirror_sha256.is_none());
    }

    #[test]
    fn a_channel_the_manifest_omits_falls_back() {
        assert!(parse(GOOD, Channel::Stable).is_none());
    }

    #[test]
    fn future_schema_is_ignored_rather_than_guessed_at() {
        let future = GOOD.replace("\"schemaVersion\": 1", "\"schemaVersion\": 99");
        assert!(parse(&future, Channel::Experimental).is_none());
    }

    #[test]
    fn garbage_falls_back() {
        for text in ["", "not json", "{}", r#"{"schemaVersion":1}"#] {
            assert!(parse(text, Channel::Experimental).is_none(), "{text:?}");
        }
    }

    /// A truncated or mistyped commit would send the installer after
    /// gigabytes that don't exist. Reject the whole manifest instead.
    #[test]
    fn a_malformed_commit_rejects_the_manifest() {
        let bad = GOOD.replace("131fd4ea4e89fd0402082cd7e4851f1836091908", "131fd4ea");
        assert!(parse(&bad, Channel::Experimental).is_none());
    }

    /// A bad digest must not block the install — drop the digest, keep the pin,
    /// and fall back to trusting the commit SHA.
    #[test]
    fn a_malformed_digest_drops_only_the_digest() {
        let bad = GOOD.replace(
            "34021b9555e42d97cb9d1383b09fa437821caba9dc848bfe446be96150fb01c0",
            "nope",
        );
        let parts = parse(&bad, Channel::Experimental).expect("should still parse");
        assert!(parts[0].mirror_sha256.is_none());
        assert_eq!(parts[0].build_id(), "131fd4ea");
    }

    #[test]
    fn built_in_pins_are_always_available() {
        for ch in [Channel::Stable, Channel::Experimental] {
            let pins = ch.built_in_pins();
            assert!(!pins.is_empty(), "{ch:?} has no fallback pins");
            for p in &pins {
                assert!(is_hex(&p.sha, 40), "{ch:?} built-in sha malformed");
            }
        }
    }

    /// The manifest that actually ships must parse. Without this the file and
    /// the parser can drift apart, and the failure would only show up as every
    /// client silently falling back to built-in pins — the exact split the
    /// manifest exists to prevent.
    #[test]
    fn the_committed_manifest_parses() {
        let text = include_str!("../../pinned-build.json");
        for ch in [Channel::Stable, Channel::Experimental] {
            let parts = parse(text, ch)
                .unwrap_or_else(|| panic!("pinned-build.json has no usable {ch:?} channel"));
            assert!(!parts.is_empty());
            for part in &parts {
                assert!(is_hex(&part.sha, 40), "{} sha malformed", part.label);
                assert!(part.url().starts_with("https://gitlab.com/"));
            }
        }
    }

    /// What ships in the file and what is compiled in must agree at release
    /// time; they only diverge later, when the file is bumped ahead of a build.
    #[test]
    fn the_committed_manifest_matches_the_built_in_pins() {
        let text = include_str!("../../pinned-build.json");
        for ch in [Channel::Stable, Channel::Experimental] {
            let published = parse(text, ch).expect("channel present");
            assert_eq!(published, ch.built_in_pins(), "{ch:?} pins have drifted");
        }
    }

    #[test]
    fn pin_source_is_reportable() {
        assert_ne!(
            PinSource::Published.describe(),
            PinSource::BuiltInOffline.describe()
        );
    }
}
