use strum::{Display, EnumIter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, Display)]
pub enum MetricCategory {
    Metadata,
    Stability,
    Usage,
    Community,
    Activity,
    Documentation,
    Trustworthiness,
    Codebase,
    Advisories,
}
