use super::common::{Common, CommonArgs};
use cargo_rank::Result;
use cargo_rank::facts::CrateRef;
use cargo_rank::misc::DependencyType;
use clap::Parser;

#[derive(Parser, Debug)]
pub struct CratesArgs {
    /// Crates to analyze (format: `crate_name` or `crate_name@version`)
    #[arg(value_name = "CRATE")]
    pub crates: Vec<CrateRef>,

    /// Dependency type to assign to the crates [default: standard]
    #[arg(long, value_name = "TYPE", default_value = "standard")]
    pub dependency_type: DependencyType,

    #[command(flatten)]
    pub common: CommonArgs,
}

pub async fn process_crates(args: &CratesArgs) -> Result<()> {
    let common = Common::new(&args.common).await?;
    let crate_facts = common.process_crates(args.crates.clone()).await?;

    common.report(crate_facts.into_iter().map(|facts| (facts, args.dependency_type)))
}
