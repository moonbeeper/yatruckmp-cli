use clap::{
    Parser,
    builder::{IntoResettable, StyledStr},
};

use crate::{errors::TResult, game::Game};

mod kill;
mod run;
mod update;

struct HelpTemplate;

impl IntoResettable<StyledStr> for HelpTemplate {
    fn into_resettable(self) -> clap::builder::Resettable<StyledStr> {
        color_print::cstr!(
            r#"<bold><underline>{name} {version}</underline></bold>
<dim>{tab}{author}</dim>

{about}

{usage-heading}
{tab}{usage}

{all-args}{after-help}
"#
        )
        .into_resettable()
    }
}

pub trait Run {
    async fn run(&self) -> TResult<()>;
}

#[derive(Debug, Parser)]
#[clap(version, about, author, propagate_version = true, help_template = HelpTemplate)]
pub enum Cmd {
    Update(Update),
    Run(RunGame),
    Kill(Kill),
}

impl Run for Cmd {
    async fn run(&self) -> TResult<()> {
        match self {
            Cmd::Update(update) => update.run().await,
            Cmd::Run(run) => run.run().await,
            Cmd::Kill(kill) => kill.run().await,
        }
    }
}

/// Update the TruckersMP mod files
#[derive(Debug, Parser, Default)]
#[clap(author, help_template = HelpTemplate)]
pub struct Update {
    /// The game to be updated
    #[clap(short, long, value_enum)]
    game: Option<Game>,

    /// Whether to clean the mod files directory before updating
    #[clap(short, long, default_value_t = false)]
    clean: bool,

    /// Whether to not verify the mod files after updating them
    #[clap(short = 'v', long, default_value_t = false)]
    no_verify: bool,

    #[clap(short, long, default_value_t = false)]
    /// Whether to not retry failed downloads.
    no_retry: bool,

    /// The number of retries to do before giving up on downloading a file.
    #[clap(short, long, default_value_t = 3)]
    retry_count: u32,
}

/// Run the TruckersMP mod for the optionally specified game
#[derive(Debug, Parser)]
#[clap(author, help_template = HelpTemplate, name = "run")]
pub struct RunGame {
    /// The game to be played
    #[clap(short, long, value_enum)]
    game: Option<Game>,
    /// Whether to not verify the mod files before running the game
    #[clap(short = 'v', long, default_value_t = false)]
    no_verify: bool,
}

/// Kill a game process if its running
#[derive(Debug, Parser)]
#[clap(author, help_template = HelpTemplate)]
pub struct Kill {
    /// The game to be killed
    #[clap(short, long, value_enum)]
    game: Option<Game>,
}
