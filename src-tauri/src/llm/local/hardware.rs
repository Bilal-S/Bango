//! Hardware capability assessment for Bango AI: target support, CPU cores,
//! total/available RAM, free disk, and AVX2 on x86. The verdict is pure and
//! injectable; detection is a thin platform probe.

use serde::Serialize;

use crate::local_ai::download::available_bytes;
use crate::local_ai::manifest::current_target;

/// Total RAM below this is a warning (not a hard gate): the 9B Q4 model can
/// still run, but slowly or under memory pressure.
pub const MIN_RAM_MB: u64 = 16 * 1024;

/// Available RAM below this is a warning alongside the total-RAM floor.
pub const LOW_AVAILABLE_RAM_MB: u64 = 8 * 1024;

/// A point-in-time hardware profile for the Bango AI panel and verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareProfile {
    /// Target identifier (`win-x64` | `osx-arm64` | `linux-x64`); `None` on
    /// unsupported machines.
    pub target: Option<String>,
    pub cpu_cores: usize,
    pub total_ram_mb: u64,
    pub available_ram_mb: u64,
    pub available_disk_mb: u64,
    /// AVX2 availability on x86; `None` on non-x86 targets (not applicable).
    pub avx2: Option<bool>,
}

/// Installation verdict: unsupported is fatal, warning is advisory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum HardwareVerdict {
    Supported,
    Warning { reasons: Vec<String> },
    Unsupported { reasons: Vec<String> },
}

impl HardwareVerdict {
    /// Whether the install may proceed (warnings still install).
    #[must_use]
    pub fn installable(&self) -> bool {
        !matches!(self, Self::Unsupported { .. })
    }

    /// Human-readable reasons for warnings/unsupported verdicts.
    #[must_use]
    pub fn reasons(&self) -> &[String] {
        match self {
            Self::Supported => &[],
            Self::Warning { reasons } | Self::Unsupported { reasons } => reasons,
        }
    }
}

/// Assess a hardware profile against the disk the install requires.
/// Unsupported = unsupported target or insufficient free disk; warning =
/// total RAM below 16 GB, little available RAM, or missing AVX2 on x86.
#[must_use]
pub fn assess(profile: &HardwareProfile, required_disk_mb: u64) -> HardwareVerdict {
    let mut unsupported = Vec::new();
    if profile.target.is_none() {
        unsupported.push(
            "Bango AI is not available for this system (supported: Windows x64, Apple \
             Silicon macOS, Linux x64)."
                .to_string(),
        );
    }
    if profile.available_disk_mb < required_disk_mb {
        unsupported.push(format!(
            "Not enough free disk space: {} GB available, {} GB required.",
            profile.available_disk_mb / 1024,
            required_disk_mb / 1024
        ));
    }
    if !unsupported.is_empty() {
        return HardwareVerdict::Unsupported { reasons: unsupported };
    }
    let mut warnings = Vec::new();
    if profile.total_ram_mb < MIN_RAM_MB {
        warnings.push(format!(
            "This computer has {} GB of memory. Bango AI needs about 8 GB for the model and \
             will be slow or may fail.",
            profile.total_ram_mb / 1024
        ));
    }
    if profile.available_ram_mb < LOW_AVAILABLE_RAM_MB {
        warnings.push(format!(
            "Only {} GB of memory is free right now; close other apps before running Bango AI.",
            profile.available_ram_mb / 1024
        ));
    }
    if profile.avx2 == Some(false) {
        warnings.push(
            "The CPU does not report AVX2 support; the local engine may not run.".to_string(),
        );
    }
    if warnings.is_empty() {
        HardwareVerdict::Supported
    } else {
        HardwareVerdict::Warning { reasons: warnings }
    }
}

/// Detect this machine's profile (`required_disk_mb` is not used here; the
/// caller assesses separately so disk can be re-probed closer to install).
#[must_use]
pub fn detect() -> HardwareProfile {
    let (total_bytes, available_resident) = {
        use sysinfo::System;
        let mut system = System::new();
        system.refresh_memory();
        (system.total_memory(), system.available_memory())
    };
    let disk_path = dirs::data_local_dir().unwrap_or_else(std::env::temp_dir);
    HardwareProfile {
        target: current_target().map(str::to_string),
        cpu_cores: std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get),
        total_ram_mb: total_bytes / (1024 * 1024),
        available_ram_mb: available_resident / (1024 * 1024),
        available_disk_mb: available_bytes(&disk_path).map_or(u64::MAX, |b| b / (1024 * 1024)),
        avx2: if cfg!(target_arch = "x86_64") {
            Some(std::arch::is_x86_feature_detected!("avx2"))
        } else {
            None
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_profile() -> HardwareProfile {
        HardwareProfile {
            target: Some("linux-x64".to_string()),
            cpu_cores: 8,
            total_ram_mb: 32 * 1024,
            available_ram_mb: 16 * 1024,
            available_disk_mb: 64 * 1024,
            avx2: Some(true),
        }
    }

    #[test]
    fn verdict_warns_below_ram_floor_and_without_avx2() {
        let mut profile = base_profile();
        profile.total_ram_mb = 8 * 1024;
        profile.available_ram_mb = 4 * 1024;
        profile.avx2 = Some(false);
        let verdict = assess(&profile, 12 * 1024);
        let HardwareVerdict::Warning { reasons } = &verdict else {
            panic!("expected a warning verdict, got {verdict:?}");
        };
        assert_eq!(reasons.len(), 3, "total RAM, available RAM, and AVX2 reasons");
        assert!(verdict.installable(), "warnings still install");
    }

    #[test]
    fn verdict_unsupported_without_disk_headroom_or_target() {
        let mut profile = base_profile();
        let supported = assess(&profile, 12 * 1024);
        assert_eq!(supported, HardwareVerdict::Supported);

        profile.available_disk_mb = 2 * 1024;
        let verdict = assess(&profile, 12 * 1024);
        assert!(!verdict.installable());
        assert!(verdict.reasons()[0].contains("free disk space"));

        let mut unsupported = base_profile();
        unsupported.target = None;
        let verdict = assess(&unsupported, 12 * 1024);
        assert!(!verdict.installable());
        assert!(verdict.reasons()[0].contains("not available for this system"));
    }
}
