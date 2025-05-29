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
    /// Export CRX files
    Export {
        /// Input CRX file
        input: String,
        /// Output path to exported PNG file
        output: String,
    },
    /// Import PNG to CRX files
    Import {
        /// Original CRX file
        origin: String,
        /// PNG file to import
        input: String,
        /// Output path for the new CRX file
        output: String,
    },
    Unpack {
        /// Input PCK file to unpack
        input: String,
        /// Output directory for unpacked files
        output: String,
    },
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
