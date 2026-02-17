use rustsec::advisory::{Informational, Severity};
use serde::{Deserialize, Serialize};

/// Advisory counts for vulnerabilities and warnings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[expect(clippy::struct_field_names, reason = "all fields represent counts, suffix improves clarity")]
pub struct AdvisoryCounts {
    pub low_vulnerability_count: u64,
    pub medium_vulnerability_count: u64,
    pub high_vulnerability_count: u64,
    pub critical_vulnerability_count: u64,

    pub notice_warning_count: u64,
    pub unmaintained_warning_count: u64,
    pub unsound_warning_count: u64,
    pub yanked_warning_count: u64,
}

/// Security advisory data for a crate.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdvisoryData {
    /// Advisory counts for the specific version being queried.
    pub per_version: AdvisoryCounts,

    /// Advisory counts across all versions of the crate (historical).
    pub total: AdvisoryCounts,
}

impl AdvisoryCounts {
    /// Apply an advisory to the counts.
    fn count_advisory(&mut self, advisory: &rustsec::Advisory) {
        if let Some(informational) = &advisory.metadata.informational {
            match informational {
                Informational::Notice => self.notice_warning_count += 1,
                Informational::Unmaintained => self.unmaintained_warning_count += 1,
                Informational::Unsound => self.unsound_warning_count += 1,
                // Note: yanked_warning_count is not used as rustsec doesn't provide yanked as an Informational type
                _ => {}
            }
            return;
        }

        if let Some(cvss) = &advisory.metadata.cvss {
            match cvss.severity() {
                Severity::None => {}
                Severity::Low => self.low_vulnerability_count += 1,
                Severity::Medium => self.medium_vulnerability_count += 1,
                Severity::High => self.high_vulnerability_count += 1,
                Severity::Critical => self.critical_vulnerability_count += 1,
            }
        }
    }
}

impl AdvisoryData {
    /// Count an advisory affecting the specific version being queried.
    pub(super) fn count_advisory_for_version(&mut self, advisory: &rustsec::Advisory) {
        self.per_version.count_advisory(advisory);
    }

    /// Count an advisory across all versions (historical).
    pub(super) fn count_advisory_historical(&mut self, advisory: &rustsec::Advisory) {
        self.total.count_advisory(advisory);
    }
}
