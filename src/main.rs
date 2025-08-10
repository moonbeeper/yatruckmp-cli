use std::{io, io::Write as _, process::ExitCode};

use clap::Parser;
use color_print::cwriteln;

use crate::cmd::{Cmd, Run};

mod cmd;
mod errors;
mod game;

#[tokio::main]
async fn main() -> ExitCode {
    _ = nu_ansi_term::enable_ansi_support();
    match Cmd::parse().run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            _ = cwriteln!(io::stderr(), "<red,bold>error</>: {e:}");
            ExitCode::FAILURE
        }
    }
}
