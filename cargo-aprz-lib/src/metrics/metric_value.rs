use chrono::{DateTime, Utc};
use compact_str::CompactString;

#[derive(Debug, Clone)]
pub enum MetricValue {
    UInt(u64),
    Float(f64),
    String(CompactString),
    Boolean(bool),
    DateTime(DateTime<Utc>),
    List(Vec<Self>),
}
