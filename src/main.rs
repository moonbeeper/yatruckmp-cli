use std::{io, io::Write as _, process::ExitCode};

use clap::Parser;
use once_cell::sync::Lazy;
use steamworks::{AppId, Client};

use crate::cmd::{Cmd, Run};

mod cmd;
mod errors;
mod game;

static STEAMWORKS_CLIENT: Lazy<Client> = Lazy::new(|| {
    // app id 480 is the safe bet as its the sdk demo app
    Client::init_app(AppId(480)).expect("Failed to initialize Steamworks client")
});
#[tokio::main]
async fn main() -> ExitCode {
    let _ = nu_ansi_term::enable_ansi_support();
    match Cmd::parse().run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            _ = writeln!(io::stderr(), "Error: {e:}");
            ExitCode::FAILURE
        }
    }
}
