use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use semver::Version;

#[derive(Debug)]
pub struct RowReader<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> RowReader<'a> {
    pub const fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    #[must_use]
    pub const fn position(&self) -> usize {
        self.position
    }

    pub fn read_byte(&mut self) -> u8 {
        let byte = self.data[self.position];
        self.position += 1;
        byte
    }

    pub fn read_u64(&mut self) -> u64 {
        let (value, bytes) = vlen::decode::<u64>(&self.data[self.position..]).expect("valid vlen u64");
        self.position += bytes;
        value
    }

    pub fn read_bool(&mut self) -> bool {
        self.read_byte() != 0
    }

    pub fn read_str(&mut self) -> &'a str {
        let len = self.read_u64();
        let len = usize::try_from(len).expect("string length fits in usize");
        let end = self.position.checked_add(len).expect("no overflow in read_bytes");
        let bytes = &self.data[self.position..end];
        self.position = end;
        core::str::from_utf8(bytes).expect("valid UTF-8 in string")
    }

    pub fn read_optional_u64(&mut self) -> Option<u64> {
        (self.read_byte() != 0).then(|| self.read_u64())
    }

    #[cfg(all_fields)]
    pub fn read_optional_str(&mut self) -> Option<&'a str> {
        (self.read_byte() != 0).then(|| self.read_str())
    }

    pub fn read_datetime(&mut self) -> DateTime<Utc> {
        let timestamp = self.read_u64();
        let timestamp = i64::try_from(timestamp).expect("timestamp in range");
        Utc.timestamp_opt(timestamp, 0).single().expect("valid timestamp")
    }

    pub fn read_date(&mut self) -> NaiveDate {
        let days = self.read_u64();
        let days = i32::try_from(days).unwrap_or(0);
        NaiveDate::from_epoch_days(days).unwrap_or_default()
    }

    pub fn read_version(&mut self) -> Version {
        let major = self.read_u64();
        let minor = self.read_u64();
        let patch = self.read_u64();
        let pre = self.read_str();
        let build = self.read_str();

        if pre.is_empty() && build.is_empty() {
            Version::new(major, minor, patch)
        } else {
            let version_str = if !pre.is_empty() && !build.is_empty() {
                format!("{major}.{minor}.{patch}-{pre}+{build}")
            } else if !pre.is_empty() {
                format!("{major}.{minor}.{patch}-{pre}")
            } else {
                format!("{major}.{minor}.{patch}+{build}")
            };
            Version::parse(&version_str).expect("valid version string")
        }
    }

    pub fn read_str_vec(&mut self) -> Vec<&'a str> {
        let count = usize::try_from(self.read_u64()).expect("vector count fits in usize");
        let mut result = Vec::with_capacity(count);
        for _ in 0..count {
            result.push(self.read_str());
        }
        result
    }
}
