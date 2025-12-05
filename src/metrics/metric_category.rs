use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, Display)]
#[serde(rename_all = "snake_case")]
pub enum MetricCategory {
    Stability,
    Usage,
    Community,
    Activity,
    Documentation,
    Ownership,
    Trustworthiness,
    Cost,
    Advisories,
}
