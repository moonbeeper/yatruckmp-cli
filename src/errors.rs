use steamworks::{SteamAPIInitError, SteamError};

use crate::game::Game;

pub type TResult<T> = std::result::Result<T, Error>;

// i love my error messages.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Steam is not running. Pretty please start Steam and try again!")]
    SteamNotRunning,
    #[error(
        "Steam seems to be running an outdated version. Are you sure you have the LATEST version?"
    )]
    SteamIsOutdated,
    #[error("Unknown Steamworks error: {0}")]
    UnknownSteamworksError(#[from] SteamError),
    #[error("{0:?} is not installed! How do you expect to play it!?")]
    SpecificGameNotInstalled(Game),
    #[error("Seems like you don't own {0:?} on steam")]
    SpecificGameNotOwned(Game),
    #[error("Neither ETS2 nor ATS are installed")]
    GamesNotInstalled,
    // #[error("Seems like you don't own neither ETS2 or ATS")]
    // GamesNotOwned,
    #[error("Sadly, we failed to launch game process in a suspended state")]
    FailedGameLaunch,
    #[error("Crap! DLL injection has failed: {0}")]
    FailedInjectingDLL(String),
    #[error("Our reqwest client has gone crazy and failed with: {0}")]
    ReqwestMiddlewareClientError(#[from] reqwest_middleware::Error),
    #[error("Our reqwest client has gone crazy and failed with: {0}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("IO error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("Somehow we failed to adquire a semaphore ticket to work on download jobs")]
    SemaphoreAcquireError(#[from] tokio::sync::AcquireError),
    #[error("Couldn't join a Tokio task... smh: {0}")]
    TokioJoinError(#[from] tokio::task::JoinError),
    #[error("Somehow we couldn't find your roaming AppData folder... how?")]
    NoAppdataPath,
    #[error("Somehow we couldn't find the {0:?} executable in its directory")]
    GameExecutableNotFound(Game),
}

impl From<SteamAPIInitError> for Error {
    fn from(value: SteamAPIInitError) -> Self {
        match value {
            SteamAPIInitError::FailedGeneric(_) => {
                Error::UnknownSteamworksError(SteamError::Generic)
            }
            SteamAPIInitError::NoSteamClient(_) => Error::SteamNotRunning,
            SteamAPIInitError::VersionMismatch(_) => Error::SteamIsOutdated,
        }
    }
}
