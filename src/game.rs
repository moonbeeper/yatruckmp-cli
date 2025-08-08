use std::path::{Path, PathBuf};

use clap::ValueEnum;
use steamworks::{AppId, Client};

use crate::errors::{Error, TResult};

#[derive(PartialEq, Eq, Clone, Copy, Debug, ValueEnum)]
pub enum Game {
    ETS2 = 227300,
    ATS = 270880,
}

impl From<Game> for AppId {
    fn from(value: Game) -> Self {
        AppId(value as u32)
    }
}

impl Game {
    pub fn exe(&self) -> &'static str {
        match self {
            Game::ETS2 => "eurotrucks2.exe",
            Game::ATS => "amtrucks.exe",
        }
    }

    pub fn dll(&self) -> &'static str {
        match self {
            Game::ETS2 => "core_ets2mp.dll",
            Game::ATS => "core_atsmp.dll",
        }
    }
}

#[derive(Default)]
struct AvailableGames {
    ets2: bool,
    ats: bool,
}

pub fn get_available_game(client: &Client) -> TResult<Game> {
    let available_games = get_games(client)?;

    match available_games {
        AvailableGames { ets2: true, .. } => Ok(Game::ETS2),
        AvailableGames { ats: true, .. } => Ok(Game::ATS),
        _ => Err(Error::GamesNotInstalled),
    }
}

pub fn get_specific_game(client: &Client, game: Game) -> TResult<Game> {
    if client.apps().is_subscribed_app(game.into()) {
        if !client.apps().is_app_installed(game.into()) {
            println!("{:?} is not installed", game);
            return Err(Error::SpecificGameNotInstalled(game));
        }

        return Ok(game);
    }
    Err(Error::SpecificGameNotOwned(game))
}

fn get_games(client: &Client) -> TResult<AvailableGames> {
    let mut result = AvailableGames::default();
    let mut counter = 0;
    if client.apps().is_subscribed_app(Game::ETS2.into()) {
        if client.apps().is_app_installed(Game::ETS2.into()) {
            result.ets2 = true;
        } else {
            counter += 1;
        }
    }
    if client.apps().is_subscribed_app(Game::ATS.into()) {
        if client.apps().is_app_installed(Game::ATS.into()) {
            result.ats = true;
        } else {
            counter += 1;
        }
    }

    if counter == 2 {
        return Err(Error::GamesNotInstalled);
    }

    Ok(result)
}

pub fn get_game_path(client: &Client, game: Game) -> TResult<PathBuf> {
    let game_dir = client.apps().app_install_dir(game.into());
    println!("game_dir: {:?}", game_dir);

    Ok(Path::new(&game_dir)
        .join("bin")
        .join("win_x64")
        .join(game.exe()))
}
