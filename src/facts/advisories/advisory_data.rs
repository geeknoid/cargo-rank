use chrono::{DateTime, Utc};
use rustsec::advisory::{Informational, Severity};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdvisoryData {
    pub timestamp: DateTime<Utc>,
    pub vulnerability_count: u64,
    pub low_vulnerability_count: u64,
    pub medium_vulnerability_count: u64,
    pub high_vulnerability_count: u64,
    pub critical_vulnerability_count: u64,
    pub warning_count: u64,
    pub notice_warning_count: u64,
    pub unmaintained_warning_count: u64,
    pub unsound_warning_count: u64,
    pub yanked_warning_count: u64,
    pub historical_vulnerability_count: u64,
    pub historical_low_vulnerability_count: u64,
    pub historical_medium_vulnerability_count: u64,
    pub historical_high_vulnerability_count: u64,
    pub historical_critical_vulnerability_count: u64,
    pub historical_warning_count: u64,
    pub historical_notice_warning_count: u64,
    pub historical_unmaintained_warning_count: u64,
    pub historical_unsound_warning_count: u64,
    pub historical_yanked_warning_count: u64,
}

impl AdvisoryData {
    pub(super) fn count_advisory_for_version(&mut self, advisory: &rustsec::Advisory) {
        let mut warning_counts = [
            &mut self.warning_count,
            &mut self.notice_warning_count,
            &mut self.unmaintained_warning_count,
            &mut self.unsound_warning_count,
        ];
        let mut vulnerability_counts = [
            &mut self.vulnerability_count,
            &mut self.low_vulnerability_count,
            &mut self.medium_vulnerability_count,
            &mut self.high_vulnerability_count,
            &mut self.critical_vulnerability_count,
        ];
        Self::apply_advisory_counts(advisory, &mut warning_counts, &mut vulnerability_counts);
    }

    pub(super) fn count_advisory_historical(&mut self, advisory: &rustsec::Advisory) {
        let mut warning_counts = [
            &mut self.historical_warning_count,
            &mut self.historical_notice_warning_count,
            &mut self.historical_unmaintained_warning_count,
            &mut self.historical_unsound_warning_count,
        ];
        let mut vulnerability_counts = [
            &mut self.historical_vulnerability_count,
            &mut self.historical_low_vulnerability_count,
            &mut self.historical_medium_vulnerability_count,
            &mut self.historical_high_vulnerability_count,
            &mut self.historical_critical_vulnerability_count,
        ];
        Self::apply_advisory_counts(advisory, &mut warning_counts, &mut vulnerability_counts);
    }

    fn apply_advisory_counts(advisory: &rustsec::Advisory, warning_counts: &mut [&mut u64; 4], vulnerability_counts: &mut [&mut u64; 5]) {
        if let Some(informational) = &advisory.metadata.informational {
            *warning_counts[0] += 1; // total warning count
            match informational {
                Informational::Notice => *warning_counts[1] += 1,
                Informational::Unmaintained => *warning_counts[2] += 1,
                Informational::Unsound => *warning_counts[3] += 1,
                // Note: yanked_warning_count is not used as rustsec doesn't provide yanked as an Informational type
                _ => {}
            }
            return;
        }

        *vulnerability_counts[0] += 1; // total vulnerability count

        if let Some(cvss) = &advisory.metadata.cvss {
            match cvss.severity() {
                Severity::None => {}
                Severity::Low => *vulnerability_counts[1] += 1,
                Severity::Medium => *vulnerability_counts[2] += 1,
                Severity::High => *vulnerability_counts[3] += 1,
                Severity::Critical => *vulnerability_counts[4] += 1,
            }
        }
    }
}
