#[derive(clap::Parser)]
pub struct Args {
    /// Path to the configuration file
    #[arg(short, long)]
    pub config: Option<String>,
}

impl Args {
    pub fn parse() -> Self {
        clap::Parser::parse()
    }
}
