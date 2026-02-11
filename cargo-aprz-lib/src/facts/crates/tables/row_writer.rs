use crate::Result;
use chrono::{DateTime, Utc};
use ohno::{IntoAppError, app_err, bail};
use semver::Version;
use std::io::Write;
use url::Url;

#[derive(Debug)]
pub struct RowWriter<'a, W: Write> {
    buffer: Vec<u8>,
    writer: &'a mut W,
    row_count: u64,
}

impl<'a, W: Write> RowWriter<'a, W> {
    pub const fn new(writer: &'a mut W) -> Self {
        Self {
            buffer: Vec::new(),
            writer,
            row_count: 0,
        }
    }

    #[must_use]
    pub const fn row_count(&self) -> u64 {
        self.row_count
    }

    pub fn row_done(&mut self) -> Result<()> {
        self.writer.write_all(&self.buffer)?;
        self.buffer.clear();
        self.row_count += 1;
        Ok(())
    }

    #[inline]
    pub fn write_byte(&mut self, byte: u8) {
        self.buffer.push(byte);
    }

    #[inline]
    pub fn write_u64(&mut self, value: u64) -> Result<()> {
        let mut buf = [0u8; 17];
        let bytes_written = vlen::encode(&mut buf[..], value).map_err(|e| app_err!("unable to encode vlen u64: {e}"))?;
        self.buffer.extend_from_slice(&buf[..bytes_written]);
        Ok(())
    }

    pub fn write_str(&mut self, s: &str) -> Result<()> {
        self.write_u64(s.len() as u64)?;
        self.buffer.extend_from_slice(s.as_bytes());
        Ok(())
    }

    #[inline]
    pub fn write_bool(&mut self, value: bool) {
        self.buffer.push(u8::from(value));
    }

    pub fn write_optional_u64(&mut self, value: Option<u64>) -> Result<()> {
        if let Some(v) = value {
            self.write_byte(1);
            self.write_u64(v)
        } else {
            self.write_byte(0);
            Ok(())
        }
    }

    #[cfg(all_fields)]
    pub fn write_optional_str(&mut self, s: &str) -> Result<()> {
        if s.is_empty() {
            self.write_byte(0);
            Ok(())
        } else {
            self.write_byte(1);
            self.write_str(s)
        }
    }

    #[cfg(all_fields)]
    pub fn write_optional_bool(&mut self, value: Option<bool>) {
        let byte = match value {
            None => 0,
            Some(false) => 1,
            Some(true) => 2,
        };
        self.buffer.push(byte);
    }

    pub fn write_str_as_u64(&mut self, s: &str) -> Result<()> {
        let value = s.parse::<u64>().into_app_err_with(|| format!("unable to parse u64 from '{s}'"))?;
        self.write_u64(value)
    }

    #[cfg(all_fields)]
    pub fn write_str_as_byte(&mut self, s: &str) -> Result<()> {
        let value = s.parse().into_app_err_with(|| format!("unable to parse u8 from '{s}'"))?;
        self.write_byte(value);
        Ok(())
    }

    pub fn write_str_as_datetime(&mut self, s: &str) -> Result<()> {
        let timestamp = parse_pg_timestamp(s)?;
        self.write_u64(timestamp)?;
        Ok(())
    }

    pub fn write_str_as_date(&mut self, s: &str) -> Result<()> {
        let timestamp = parse_pg_date(s)?;
        self.write_u64(timestamp)
    }

    pub fn write_str_as_url(&mut self, s: &str) -> Result<()> {
        if s.is_empty() {
            return self.write_str(s);
        }

        // Try parsing the URL as-is
        if Url::parse(s).is_ok() {
            return self.write_str(s);
        }

        // If that fails, try prepending https://
        let with_https = format!("https://{s}");
        if Url::parse(&with_https).is_ok() {
            return self.write_str(&with_https);
        }

        // Both attempts failed, return error
        bail!("unable to parse URL from '{s}'");
    }

    pub fn write_optional_str_as_u64(&mut self, s: &str) -> Result<()> {
        if s.is_empty() {
            self.write_byte(0);
            return Ok(());
        }

        let v = s.parse::<u64>().into_app_err_with(|| format!("unable to parse u64 from '{s}'"))?;
        self.write_optional_u64(Some(v))
    }

    pub fn write_str_as_bool(&mut self, s: &str) -> Result<()> {
        let value = match s {
            "t" | "true" => true,
            "f" | "false" | "" => false,
            _ => bail!("invalid boolean value: expected 't', 'true', 'f', 'false', or empty, got '{s}'"),
        };

        self.write_bool(value);
        Ok(())
    }

    pub fn write_str_as_version(&mut self, s: &str) -> Result<()> {
        let version = Version::parse(s).into_app_err_with(|| format!("unable to parse version '{s}'"))?;
        self.write_u64(version.major)?;
        self.write_u64(version.minor)?;
        self.write_u64(version.patch)?;
        self.write_str(version.pre.as_str())?;
        self.write_str(version.build.as_str())?;
        Ok(())
    }

    pub fn write_pg_array_as_str_vec(&mut self, s: &str) -> Result<()> {
        let inner = s
            .strip_prefix('{')
            .and_then(|s| s.strip_suffix('}'))
            .ok_or_else(|| app_err!("invalid PostgreSQL array format: expected '{{...}}', got '{s}'"))?;

        if inner.is_empty() {
            self.write_u64(0)
        } else {
            let count = inner.matches(',').count() + 1;
            self.write_u64(count as u64)?;
            for element in inner.split(',') {
                self.write_str(element)?;
            }
            Ok(())
        }
    }
}

fn parse_pg_timestamp(s: &str) -> Result<u64> {
    let dt = DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f%#z")
        .or_else(|_| DateTime::parse_from_rfc3339(s))
        .into_app_err_with(|| format!("unable to parse timestamp '{s}'"))?
        .with_timezone(&Utc);
    Ok(dt.timestamp().max(0).cast_unsigned())
}

fn parse_pg_date(s: &str) -> Result<u64> {
    Ok(u64::from(
        chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .into_app_err_with(|| format!("unable to parse date '{s}'"))?
            .to_epoch_days()
            .max(0)
            .cast_unsigned(),
    ))
}

