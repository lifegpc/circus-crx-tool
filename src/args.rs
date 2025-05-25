use clap::{Parser, Subcommand};

/// Tools for export and import CIRCUS CRX files.
#[derive(Parser, Debug)]
#[clap(args_conflicts_with_subcommands = true)]
#[command(version, about, long_about = None)]
pub struct Arg {
    #[clap(subcommand)]
    /// Subcommands
    pub command: Option<Command>,
    #[clap(flatten)]
    pub auto: Option<AutoArgs>,
}

#[derive(Subcommand, Debug)]
/// Commands
pub enum Command {
    Export { input: String, output: String },
}

#[derive(Parser, Debug)]
pub struct AutoArgs {
    /// Export/Import CRX files
    pub input: String,
}

impl Arg {
    /// Parse command line arguments
    pub fn parse() -> Self {
        clap::Parser::parse()
    }
}
