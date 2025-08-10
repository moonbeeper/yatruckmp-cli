use clap::{
    Parser, Subcommand,
    builder::{IntoResettable, StyledStr},
};

use crate::{cmd::game::server::ServerInfoType, errors::TResult, game::Game};

mod game;
mod kill;
mod mod_version;
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
    Version(ModVersion),
    Game(GameCmd),
}

impl Run for Cmd {
    async fn run(&self) -> TResult<()> {
        match self {
            Cmd::Update(update) => update.run().await,
            Cmd::Run(run) => run.run().await,
            Cmd::Kill(kill) => kill.run().await,
            Cmd::Version(cmd) => cmd.run().await,
            Cmd::Game(cmd) => cmd.run().await,
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

/// Get the current mod version with the supported game versions
#[derive(Debug, Parser)]
#[clap(author, help_template = HelpTemplate)]
pub struct ModVersion;

/// Get game related information.
#[derive(Debug, Parser)]
#[clap(author, help_template = HelpTemplate)]
pub struct GameCmd {
    #[clap(subcommand)]
    pub cmd: Option<GameCommand>,
}

#[derive(Debug, Subcommand, Clone)]
pub enum GameCommand {
    Servers(ServerInfo),
}

/// Get the current server info
#[derive(Debug, Parser, Clone)]
#[clap(author, help_template = HelpTemplate)]
pub struct ServerInfo {
    /// The game to get the server info for
    #[clap(short, long, value_enum)]
    game: Option<Game>,
    /// The server type to get specifically info for
    #[clap(short = 't', long = "type", value_enum, default_values_t = [ServerInfoType::All])]
    server_type: Vec<ServerInfoType>,

    /// Whether to show the ping to the server
    #[clap(short, long, default_value_t = false)]
    ping: bool,

    /// Whether to show the server's speed limit
    #[clap(short, long, default_value_t = false)]
    speed_limit: bool,

    /// Whether to show additional info about the server
    #[clap(short, long, default_value_t = false)]
    additional: bool,
}
