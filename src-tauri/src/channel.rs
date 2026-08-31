//! Undead Legacy release channels.
//!
//! Subquake ships two lines at once, against different base games and with
//! genuinely different payloads:
//!
//! | Channel      | UL    | Base game | Archives        | Doorstop |
//! |--------------|-------|-----------|-----------------|----------|
//! | stable       | 2.6.x | A20.7     | one             | 3        |
//! | experimental | 2.7.x | v2.6      | Part 1 + Part 2 | 4        |
//!
//! Every archive is pinned to an exact commit rather than tracked from `main`.
//! 7 Days to Die refuses connections between a client and server on different
//! mod builds, and the 2.7 line shipped five builds in its first twelve days,
//! so "newest" would mean friends who installed on different days could not
//! play together. Updating is therefore a deliberate act: bump the SHAs here
//! *and* in `server/setup-ul-server.sh`, then reissue the app.
//!
//! The old `?v=ml_exp` key this launcher used now resolves to a 978-byte
//! LICENSE.zip; the remaining subquake catalog keys all redirect to `main`,
//! which is why none of them are used as fallbacks here.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    /// UL 2.6.x — the long-standing A20.7 release.
    Stable,
    /// UL 2.7.x — the v2.6 rewrite, updated frequently.
    #[default]
    Experimental,
}

/// Which base-game build is installed. A UL release targets exactly one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GameLine {
    /// Alpha 20.7 — the `alpha20.7` Steam beta. What UL 2.6 stable needs.
    A20,
    /// v2.6 — the `v2.6` Steam beta. What UL 2.7 experimental needs.
    ///
    /// This is *not* the default branch: Steam's default has moved on to V3.2,
    /// which UL does not support. Selecting it is a deliberate act.
    V26,
    /// A build no UL release targets — most often Steam's current default.
    Unsupported,
    /// The Steam branch and the installed files disagree, which means Steam
    /// has switched branch but not finished downloading it yet. Installing now
    /// would put a mod for one game generation onto another.
    Updating,
    /// Nothing on disk to judge by.
    Unknown,
}

/// Unity Doorstop generation. The two renamed every environment variable
/// between them, so injecting with the wrong set launches the game *unmodded*
/// and silent — no crash, just vanilla.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Doorstop {
    /// Ships `libdoorstop_x64.dylib` / `libdoorstop_x86.dylib`.
    V3,
    /// Ships a single universal `libdoorstop.dylib`.
    V4,
}

/// One downloadable archive making up a channel's package, pinned to a commit.
///
/// Pinning is not optional. 7 Days to Die refuses connections between a client
/// and server running different mod builds, and the 2.7 line publishes every
/// few days — `main` advanced during a single 40-minute download while this
/// was being written. Tracking `main` would mean two friends who install on
/// different days, or even hours apart, cannot play together.
///
/// This is also why there is no second mirror: the subquake catalog keys
/// (`?v=exp_part1`) always redirect to `main`, so falling back to one would
/// silently hand a player a *different* build. A failed download is obvious
/// and recoverable; a silently mismatched build is neither.
pub struct Part {
    /// Shown to the user, e.g. "part 1 of 2".
    pub label: &'static str,
    pub group: &'static str,
    pub repo: &'static str,
    /// Exact commit installed. Must match `server/setup-ul-server.sh`.
    pub sha: &'static str,
}

/// Private mirror on the tailnet.
///
/// Holds byte-identical copies of the pinned archives and, unlike GitLab,
/// honours `Range` — so an interrupted download resumes instead of restarting
/// from zero on a multi-gigabyte file. It is a CGNAT address that only resolves
/// inside the tailnet; anyone else simply fails to connect and falls through to
/// GitLab, which stays the source of truth.
///
/// Serving it privately rather than publishing it is deliberate: the Undead
/// Legacy licence limits redistribution to private use.
const TAILNET_MIRROR: &str = "http://100.69.65.14:8088";

/// SHA-256 of the copies stored on the private mirror.
///
/// Deliberately *not* a property of the pinned commit. GitLab generates its
/// archives on demand and the result is not byte-reproducible — two downloads
/// of this very commit differed by 261 KB in the zip wrapper alone, while the
/// contents were identical. So a digest can only describe one stored file,
/// which is exactly what the mirror holds. A GitLab download is trusted
/// through the commit SHA embedded in its URL instead.
const EXPERIMENTAL_MIRROR_DIGESTS: &[&str] = &[
    "34021b9555e42d97cb9d1383b09fa437821caba9dc848bfe446be96150fb01c0",
    "2881ea7bc9735c3cdfbff1edba7615d111f80e5bbbc0a402a904bc8e5df1f82d",
];

/// Stable is not mirrored yet — it always comes from GitLab.
const STABLE_MIRROR_DIGESTS: &[&str] = &[];

const STABLE_PARTS: &[Part] = &[Part {
    label: "the mod files",
    group: "Subquake",
    repo: "UndeadLegacyStable",
    // UL 2.6.17, 2022-11-25 — the stable line has not moved since.
    sha: "093b0380f76f3e5ad000d285f7136cbd63eabf3b",
}];

const EXPERIMENTAL_PARTS: &[Part] = &[
    Part {
        label: "part 1 of 2",
        group: "subquakesgroup",
        repo: "UndeadLegacyExperimentalPart1",
        // UL 2.7.19, 2026-08-29.
        sha: "131fd4ea4e89fd0402082cd7e4851f1836091908",
    },
    Part {
        label: "part 2 of 2",
        group: "subquakesgroup",
        repo: "UndeadLegacyExperimentalPart2",
        // Bulk assets; updated less often than part 1.
        sha: "e890a4ead20da776a0554f7b098f2e613ebaccc1",
    },
];

impl Channel {
    pub fn parts(self) -> &'static [Part] {
        match self {
            Channel::Stable => STABLE_PARTS,
            Channel::Experimental => EXPERIMENTAL_PARTS,
        }
    }

    pub fn requires(self) -> GameLine {
        match self {
            Channel::Stable => GameLine::A20,
            Channel::Experimental => GameLine::V26,
        }
    }

    pub fn accepts(self, line: GameLine) -> bool {
        line == self.requires()
    }

    /// Human label for the UL line, without a point version that would rot.
    pub fn ul_label(self) -> &'static str {
        match self {
            Channel::Stable => "Undead Legacy 2.6 (stable)",
            Channel::Experimental => "Undead Legacy 2.7 (experimental)",
        }
    }

    pub fn game_label(self) -> &'static str {
        match self {
            Channel::Stable => "Alpha 20.7",
            Channel::Experimental => "v2.6",
        }
    }

    /// What the user must pick under Steam → Properties → Betas.
    ///
    /// Both are named betas. Neither is the default branch — Steam's default
    /// is V3.2, which no UL release supports.
    pub fn steam_branch(self) -> &'static str {
        match self {
            Channel::Stable => "alpha20.7",
            Channel::Experimental => "v2.6",
        }
    }

    /// Where the private mirror keeps this channel's archives. Filenames match
    /// the local cache, so the two are trivially interchangeable.
    pub fn mirror_url(self, index: usize) -> String {
        format!("{TAILNET_MIRROR}/{}-{index}.zip", self.cache_stem())
    }

    /// The pins compiled into this binary, used when the published manifest
    /// can't be reached or doesn't parse. Always available, so a friend who is
    /// offline still gets a working install rather than an error.
    pub fn built_in_pins(self) -> Vec<crate::pin::PinnedPart> {
        self.parts()
            .iter()
            .enumerate()
            .map(|(index, p)| crate::pin::PinnedPart {
                label: p.label.to_string(),
                group: p.group.to_string(),
                repo: p.repo.to_string(),
                sha: p.sha.to_string(),
                mirror_sha256: self.mirror_digest(index).map(str::to_string),
            })
            .collect()
    }

    /// Expected digest of the mirror's copy, if we have one on record.
    pub fn mirror_digest(self, index: usize) -> Option<&'static str> {
        let digests = match self {
            Channel::Stable => STABLE_MIRROR_DIGESTS,
            Channel::Experimental => EXPERIMENTAL_MIRROR_DIGESTS,
        };
        digests.get(index).copied()
    }

    /// Cache subfolder, so channels never reuse each other's downloads.
    pub fn cache_stem(self) -> &'static str {
        match self {
            Channel::Stable => "ul-stable",
            Channel::Experimental => "ul-experimental",
        }
    }

    /// Headroom needed to download and unpack, in bytes.
    ///
    /// Measured, not estimated. Experimental is two archives of 3.09 GiB and
    /// 3.33 GiB — ~6.9 GB — and their contents are already-compressed Unity
    /// asset bundles, so the unpacked tree is about the same size again. Peak
    /// is both archives plus that tree; installing *renames* the tree into
    /// place rather than copying, so there is no second full copy.
    ///
    /// Archives are kept until the install succeeds so a failure part-way
    /// through doesn't force re-downloading gigabytes. That costs ~3 GB of
    /// peak disk and is the reason this number is as large as it is.
    pub fn required_free_bytes(self) -> u64 {
        match self {
            Channel::Stable => 5_000_000_000,
            Channel::Experimental => 15_000_000_000,
        }
    }

    /// Steam instructions phrased for this channel.
    pub fn switch_hint(self) -> String {
        format!(
            "In Steam: right-click 7 Days to Die → Properties → Betas → {}. Wait for the download to finish, then try again.",
            self.steam_branch()
        )
    }
}

impl Doorstop {
    /// dylib basenames this generation ships, in preference order.
    pub fn dylib_names(self) -> &'static [&'static str] {
        match self {
            Doorstop::V3 => &["libdoorstop_x64.dylib", "libdoorstop_x86.dylib"],
            Doorstop::V4 => &["libdoorstop.dylib"],
        }
    }

    /// Newest first: a payload carrying both should be driven as V4.
    pub fn all() -> [Doorstop; 2] {
        [Doorstop::V4, Doorstop::V3]
    }
}

/// Which UL channel suits what is installed, used to preselect the UI.
pub fn channel_for(line: GameLine) -> Option<Channel> {
    match line {
        GameLine::A20 => Some(Channel::Stable),
        GameLine::V26 => Some(Channel::Experimental),
        GameLine::Unsupported | GameLine::Updating | GameLine::Unknown => None,
    }
}

/// True when the Unity runtime says this is the Alpha 20 era.
///
/// A20.7 Mac shipped Unity 2020.3.14f1; everything since moved to 2021+. This
/// only separates A20 from "modern" — it cannot tell v2.6 from V3.2, which
/// share a Unity generation, so it is not enough on its own.
/// (`CFBundleShortVersionString` is a useless "1.0" on every build.)
pub fn unity_says_alpha20(info_plist_text: &str) -> Option<bool> {
    let idx = info_plist_text.find("Unity Player version ")?;
    let rest = &info_plist_text[idx + "Unity Player version ".len()..];
    let year: u32 = rest
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .filter(|s| s.len() == 4)?
        .parse()
        .ok()?;
    Some(year <= 2020)
}

/// Classify from the Steam branch, which is the authoritative signal.
///
/// Both builds UL supports are *named betas*. Steam's default branch is V3.2,
/// which no UL release targets — so an absent BetaKey means unsupported, not
/// "current and fine". Getting this backwards would wave players through on a
/// build that cannot join the server.
pub fn line_from_beta_key(beta_key: Option<&str>) -> GameLine {
    let Some(key) = beta_key else {
        // No BetaKey means the default branch.
        return GameLine::Unsupported;
    };
    let key = key.trim().to_ascii_lowercase();
    if key.contains("alpha20") || key.contains("20.7") {
        GameLine::A20
    } else if key.starts_with("v2.6") || key == "2.6" {
        GameLine::V26
    } else {
        GameLine::Unsupported
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unity_2020_is_the_a20_era() {
        // Verbatim from a real A20.7 install.
        let plist = "Unity Player version 2020.3.14f1 (d0d1bb862f9d). (c) 2021 Unity";
        assert_eq!(unity_says_alpha20(plist), Some(true));
    }

    #[test]
    fn newer_unity_is_not_a20() {
        let plist = "Unity Player version 2022.3.29f1 (8d91ffdec66b).";
        assert_eq!(unity_says_alpha20(plist), Some(false));
    }

    #[test]
    fn unparsable_plist_is_none() {
        assert_eq!(unity_says_alpha20("no unity here"), None);
    }

    #[test]
    fn alpha20_branch_is_a20() {
        assert_eq!(line_from_beta_key(Some("alpha20.7")), GameLine::A20);
    }

    #[test]
    fn v26_branch_is_the_experimental_target() {
        assert_eq!(line_from_beta_key(Some("v2.6")), GameLine::V26);
        assert_eq!(line_from_beta_key(Some("V2.6")), GameLine::V26);
    }

    /// The default branch is Steam's current release (V3.2 as of writing) and
    /// no UL build targets it. Treating "no BetaKey" as fine would wave players
    /// onto a build that cannot join the server — the exact bug this replaced.
    #[test]
    fn default_branch_is_unsupported() {
        assert_eq!(line_from_beta_key(None), GameLine::Unsupported);
        assert_eq!(channel_for(GameLine::Unsupported), None);
    }

    #[test]
    fn other_named_beta_is_unsupported() {
        assert_eq!(
            line_from_beta_key(Some("latest_experimental")),
            GameLine::Unsupported
        );
    }

    #[test]
    fn each_channel_names_a_real_beta_branch() {
        // Neither channel may tell players to use the default branch.
        for ch in [Channel::Stable, Channel::Experimental] {
            let branch = ch.steam_branch();
            assert!(
                !branch.to_ascii_lowercase().contains("default"),
                "{ch:?} points at the default branch"
            );
            assert_eq!(line_from_beta_key(Some(branch)), ch.requires());
        }
    }

    #[test]
    fn channels_want_different_game_lines() {
        assert!(Channel::Stable.accepts(GameLine::A20));
        assert!(!Channel::Stable.accepts(GameLine::V26));
        assert!(Channel::Experimental.accepts(GameLine::V26));
        assert!(!Channel::Experimental.accepts(GameLine::A20));
    }

    /// A branch switch that Steam has not finished is the state where the
    /// manifest and the files on disk disagree. Installing then would put a
    /// Doorstop 4 payload on an A20 game (or the reverse) and look successful.
    #[test]
    fn nothing_accepts_an_unknown_or_unsupported_game() {
        for line in [GameLine::Unknown, GameLine::Unsupported, GameLine::Updating] {
            assert!(!Channel::Stable.accepts(line));
            assert!(!Channel::Experimental.accepts(line));
        }
    }

    #[test]
    fn experimental_ships_two_parts() {
        assert_eq!(Channel::Experimental.parts().len(), 2);
        assert_eq!(Channel::Stable.parts().len(), 1);
    }

    /// Every part must name an exact commit, never a moving ref. A branch name
    /// here would let two players install different builds and be unable to
    /// join the same server.
    #[test]
    fn every_part_is_pinned_to_a_commit() {
        for ch in [Channel::Stable, Channel::Experimental] {
            for part in ch.parts() {
                assert_eq!(
                    part.sha.len(),
                    40,
                    "{} is not a full commit sha",
                    part.label
                );
                assert!(
                    part.sha.chars().all(|c| c.is_ascii_hexdigit()),
                    "{} sha is not hex",
                    part.label
                );
            }
        }
    }

    #[test]
    fn parts_within_a_channel_are_distinct() {
        let parts = Channel::Experimental.parts();
        assert_ne!(parts[0].repo, parts[1].repo);
        assert_ne!(parts[0].sha, parts[1].sha);
    }

    /// The compiled-in fallback must reproduce exactly what the static table
    /// describes, including which parts are mirrored.
    #[test]
    fn built_in_pins_mirror_the_static_table() {
        for ch in [Channel::Stable, Channel::Experimental] {
            let pins = ch.built_in_pins();
            assert_eq!(pins.len(), ch.parts().len());
            for (index, pin) in pins.iter().enumerate() {
                assert_eq!(pin.sha, ch.parts()[index].sha);
                assert_eq!(pin.mirror_sha256.as_deref(), ch.mirror_digest(index));
                assert!(pin.url().contains(&pin.sha), "resolved url dropped the pin");
                assert!(
                    !pin.url().contains("-main.zip"),
                    "resolved url points at a moving branch"
                );
            }
        }
    }

    /// The mirror serves files under the same names the local cache uses, so a
    /// file can be copied between them by hand without renaming.
    #[test]
    fn mirror_filenames_match_the_local_cache() {
        for ch in [Channel::Stable, Channel::Experimental] {
            for index in 0..ch.parts().len() {
                let expected = format!("{}-{index}.zip", ch.cache_stem());
                assert!(
                    ch.mirror_url(index).ends_with(&expected),
                    "{} should end with {expected}",
                    ch.mirror_url(index)
                );
            }
        }
    }

    /// A recorded mirror digest must be a well-formed SHA-256. A channel with
    /// no digests simply isn't mirrored and always uses GitLab.
    #[test]
    fn mirror_digests_are_well_formed() {
        for ch in [Channel::Stable, Channel::Experimental] {
            for index in 0..ch.parts().len() {
                let Some(d) = ch.mirror_digest(index) else {
                    continue;
                };
                assert_eq!(d.len(), 64, "{ch:?} part {index} digest length");
                assert!(d.chars().all(|c| c.is_ascii_hexdigit()));
                assert_eq!(d, d.to_ascii_lowercase(), "digest must be lowercase hex");
            }
        }
    }

    /// Every mirrored part needs its own digest — two parts sharing one would
    /// mean a copy/paste slip that lets the wrong file verify.
    #[test]
    fn each_mirrored_part_has_a_distinct_digest() {
        let digests: Vec<_> = (0..Channel::Experimental.parts().len())
            .filter_map(|i| Channel::Experimental.mirror_digest(i))
            .collect();
        assert_eq!(
            digests.len(),
            2,
            "both experimental parts should be mirrored"
        );
        assert_ne!(digests[0], digests[1]);
    }

    #[test]
    fn channels_do_not_share_a_cache() {
        assert_ne!(
            Channel::Stable.cache_stem(),
            Channel::Experimental.cache_stem()
        );
    }

    #[test]
    fn doorstop_generations_use_distinct_dylibs() {
        for v4 in Doorstop::V4.dylib_names() {
            assert!(!Doorstop::V3.dylib_names().contains(v4));
        }
    }

    #[test]
    fn doorstop_probe_order_prefers_v4() {
        assert_eq!(Doorstop::all()[0], Doorstop::V4);
    }
}
