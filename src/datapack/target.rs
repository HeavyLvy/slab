//! The Minecraft versions a pack can be built for, and the data pack format each one reads.
//!
//! Only 26.3 has been checked against a real server (`tools/verify_all.sh`, `docs/mc/05-verified-by-slab.md`).
//! 26.1 and 26.2 are accepted, with their formats from `docs/mc/01-datapack-structure.md`, but
//! the commands slab writes were not tried on them, so building for them says so. Versions before
//! 26.1 are refused: their `pack.mcmeta` and many commands are different.

use std::fmt;

/// A game version, as written in `Slab.toml` (`minecraft = "26.3"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    /// `"26.3"`: the release family; patch releases (`26.3.1`) read the same format.
    pub name: &'static str,
    /// Data pack format `major.minor`.
    pub format: (u32, u32),
    /// Whether slab's output was run on this version.
    pub verified: bool,
}

pub const TARGETS: &[Target] = &[
    Target { name: "26.1", format: (101, 1), verified: false },
    Target { name: "26.2", format: (107, 1), verified: false },
    Target { name: "26.3", format: (121, 0), verified: true },
];

impl Target {
    /// The newest version, the one the compiler is checked against.
    pub const LATEST: Target = TARGETS[TARGETS.len() - 1];

    /// Reads `26.3` or a patch release such as `26.1.2`.
    pub fn parse(text: &str) -> Result<Target, String> {
        let family = text.trim().splitn(3, '.').take(2).collect::<Vec<_>>().join(".");
        let patch_ok = text.trim().split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
        match TARGETS.iter().find(|t| t.name == family) {
            Some(t) if patch_ok => Ok(*t),
            _ => {
                let known: Vec<&str> = TARGETS.iter().map(|t| t.name).collect();
                Err(format!("unsupported Minecraft version `{text}`: slab builds packs for {} (older versions use a different pack.mcmeta and different commands)", known.join(", ")))
            }
        }
    }

    /// A line to show when building for a version that has not been tried.
    pub fn caveat(&self) -> Option<String> {
        (!self.verified).then(|| format!("slab is only checked against Minecraft {}; the pack for {} gets data pack format {}.{} but its commands were not tried on that version", Target::LATEST.name, self.name, self.format.0, self.format.1))
    }
}

impl Default for Target {
    fn default() -> Self {
        Target::LATEST
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_releases_and_patch_versions() {
        assert_eq!(Target::parse("26.3").unwrap().format, (121, 0));
        assert_eq!(Target::parse("26.1.2").unwrap().name, "26.1");
        assert_eq!(Target::parse(" 26.2 ").unwrap().format, (107, 1));
        for bad in ["1.21.4", "26", "26.9", "27.1", "26.3.x", "latest", ""] {
            assert!(Target::parse(bad).unwrap_err().contains("unsupported Minecraft version"), "{bad}");
        }
        assert_eq!(Target::default(), Target::parse("26.3").unwrap());
        assert!(Target::parse("26.3").unwrap().caveat().is_none());
        assert!(Target::parse("26.1").unwrap().caveat().unwrap().contains("101.1"));
    }
}
