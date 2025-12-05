mod color;
#[expect(clippy::module_inception, reason = "I like it this way")]
mod config;
mod policies;
mod policy;

pub use color::Color;
pub use config::{Config, DEFAULT_CONFIG_YAML};
pub use policies::{
    AgePolicy, AgedCountPolicy, BooleanPolicy, LicensePolicy, MaxCountPolicy, MinCountPolicy, PercentagePolicy, ResponsivenessPolicy,
    VersionPolicy,
};
pub use policy::Policy;
