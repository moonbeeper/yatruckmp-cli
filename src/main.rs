use std::{
    ffi::OsStr,
    fmt::Write,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::Arc,
};

use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressState, ProgressStyle};
use once_cell::sync::Lazy;
use steamworks::{AppId, Client, SteamAPIInitError, SteamError};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt as _, BufReader},
    sync::Semaphore,
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::{
            Diagnostics::Debug::WriteProcessMemory,
            LibraryLoader::{GetModuleHandleA, GetProcAddress},
            Memory::{MEM_COMMIT, MEM_RESERVE, PAGE_READWRITE, VirtualAllocEx},
            Threading::{
                CREATE_SUSPENDED, CreateProcessW, CreateRemoteThread, INFINITE,
                PROCESS_INFORMATION, ResumeThread, STARTUPINFOW, WaitForSingleObject,
            },
        },
    },
    core::{PWSTR, s},
};

type Result<T> = std::result::Result<T, Error>;

// i love my error messages.
#[derive(Debug, thiserror::Error)]
enum Error {
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
    #[error("Seems like you don't own {0:?}")]
    SpecificGameNotOwned(Game),
    #[error("Neither ETS2 nor ATS are installed")]
    GamesNotInstalled,
    #[error("Seems like you don't own neither ETS2 or ATS")]
    GamesNotOwned,
    #[error("Sadly, we failed to launch game process in a suspended state")]
    FailedGameLaunch,
    #[error("DLL injection has failed: {0}")]
    FailedInjectingDLL(String),
    #[error("Reqwest client error: {0}")]
    ReqwestClientError(#[from] reqwest::Error),
    #[error("IO error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("Somehow we failed to adquire a semaphore ticket to work on download jobs")]
    SemaphoreAcquireError(#[from] tokio::sync::AcquireError),
    #[error("Couldn't join a Tokio task... smh: {0}")]
    TokioJoinError(#[from] tokio::task::JoinError),
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

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum Game {
    ETS2 = 227300,
    ATS = 270880,
}

impl From<Game> for AppId {
    fn from(value: Game) -> Self {
        AppId(value as u32)
    }
}

impl Game {
    fn exe(&self) -> &'static str {
        match self {
            Game::ETS2 => "eurotrucks2.exe",
            Game::ATS => "amtrucks.exe",
        }
    }

    fn dll(&self) -> &'static str {
        match self {
            Game::ETS2 => "core_ets2mp.dll",
            Game::ATS => "core_atsmp.dll",
        }
    }
}

fn get_game(client: &Client) -> Result<Game> {
    let mut counter = 0; // great great way to handle it
    if client.apps().is_subscribed_app(Game::ETS2.into()) {
        if !client.apps().is_app_installed(Game::ETS2.into()) {
            println!("ETS2 is not installed");
            counter += 1;
        } else {
            return Ok(Game::ETS2);
        }
    } else if client.apps().is_subscribed_app(Game::ATS.into()) {
        if !client.apps().is_app_installed(Game::ATS.into()) {
            println!("ATS is not installed");
            counter += 1;
        } else {
            return Ok(Game::ATS);
        }
    }

    if counter == 2 {
        return Err(Error::GamesNotInstalled);
    }

    Err(Error::GamesNotOwned)
}

// todo: actually output the error message
#[tokio::main]
async fn main() -> Result<()> {
    let client = Client::init_app(AppId(480))?; // app id 480 is the safe bet as its the sdk demo app
    let reqwest_client = reqwest::Client::builder()
        .user_agent("Yet Another TruckersMP Cli/0.x")
        .build()?;
    let res = get_content_files(&reqwest_client).await?;
    // println!("content files: {:?}", res);
    let game = get_game(&client)?;
    let content_dir = Path::new("test/").to_path_buf();

    update_files(&reqwest_client, res, &game, content_dir.clone()).await?;

    println!("game: {:?}", game);

    let game_dir = client.apps().app_install_dir(game.into());
    println!("game_dir: {:?}", game_dir);

    let path = Path::new(&game_dir)
        .join("bin")
        .join("win_x64")
        .join(game.exe());
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect(); // uft16

    let dll_path = content_dir.join(game.dll());

    println!("launching game");
    unsafe {
        std::env::set_var("SteamGameId", format!("{}", game as u32));
        std::env::set_var("SteamAppId", format!("{}", game as u32));

        let mut startup_info: STARTUPINFOW = std::mem::zeroed();
        startup_info.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        let mut process_info: PROCESS_INFORMATION = std::mem::zeroed();

        CreateProcessW(
            None,
            Some(PWSTR(path.as_ptr() as *mut u16)),
            None,
            None,
            false,
            CREATE_SUSPENDED,
            None,
            None,
            &startup_info,
            &mut process_info,
        )
        .map_err(|_| Error::FailedGameLaunch)?;

        inject_dll(process_info.hProcess, dll_path)?;
        ResumeThread(process_info.hThread);
        CloseHandle(process_info.hThread).ok();
    }
    Ok(())
}

// your typical remote thread dll or shellcode injection lol
fn inject_dll(process: HANDLE, dll_path: PathBuf) -> Result<()> {
    let dll_path: Vec<u16> = dll_path.as_os_str().encode_wide().chain(Some(0)).collect(); // uft16
    let dll_path_len = dll_path.len() * std::mem::size_of::<u16>();

    unsafe {
        let alloc_addr = VirtualAllocEx(
            process,
            None,
            dll_path_len,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        );

        if alloc_addr.is_null() {
            CloseHandle(process).ok();
            return Err(Error::FailedInjectingDLL(
                "failed to allocate memory in the game process".into(),
            ));
        }

        WriteProcessMemory(
            process,
            alloc_addr,
            dll_path.as_ptr() as *const _,
            dll_path_len,
            None,
        )
        .map_err(|_| {
            CloseHandle(process).ok();
            Error::FailedInjectingDLL("failed to write to game process memory".into())
        })?;

        let kernel_handle = GetModuleHandleA(s!("kernel32.dll")).map_err(|_| {
            CloseHandle(process).ok();
            Error::FailedInjectingDLL("failed to get kernel32 handle".into())
        })?;
        let load_library_addr =
            GetProcAddress(kernel_handle, s!("LoadLibraryW")).ok_or_else(|| {
                CloseHandle(process).ok();
                Error::FailedInjectingDLL("failed to get LoadLibraryW addr".into())
            })?;

        let remote_thread = CreateRemoteThread(
            process,
            None,
            0,
            // stupid type signature
            Some(std::mem::transmute(load_library_addr)),
            Some(alloc_addr),
            0,
            None,
        )
        .map_err(|_| {
            CloseHandle(process).ok();
            Error::FailedInjectingDLL("failed to create remote thread into game process".into())
        })?;

        WaitForSingleObject(remote_thread, INFINITE);
        CloseHandle(process).ok();
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct ContentFiles {
    ets2: Vec<ContentFile>,
    ats: Vec<ContentFile>,
    shared: Vec<ContentFile>,
}

impl From<RawContentFiles> for ContentFiles {
    fn from(value: RawContentFiles) -> Self {
        let mut ets2 = Vec::new();
        let mut ats = Vec::new();
        let mut shared = Vec::new();

        for file in value.files {
            match file.ctype {
                RawContentType::ETS2 => ets2.push(file.into()),
                RawContentType::ATS => ats.push(file.into()),
                RawContentType::System => shared.push(file.into()),
            }
        }

        Self { ets2, ats, shared }
    }
}

#[derive(Debug, Clone)]
struct ContentFile {
    md5: String,
    file_path: String,
}

impl From<RawContentFile> for ContentFile {
    fn from(value: RawContentFile) -> Self {
        ContentFile {
            md5: value.md5,
            file_path: value
                .file_path
                .strip_prefix("/")
                .unwrap_or(&value.file_path)
                .to_string(),
        }
    }
}

#[derive(serde::Deserialize, Debug, PartialEq, Eq, Clone, Copy)]
enum RawContentType {
    #[serde(rename = "ets2")]
    ETS2,
    #[serde(rename = "ats")]
    ATS,
    #[serde(rename = "system")]
    System,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct RawContentFile {
    #[serde(rename = "Md5")]
    md5: String,
    #[serde(rename = "Type")]
    ctype: RawContentType,
    #[serde(rename = "FilePath")]
    file_path: String,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct RawContentFiles {
    #[serde(rename = "Files")]
    files: Vec<RawContentFile>,
}

const UPDATE_URL: &str = "https://update.ets2mp.com/files.json";
const DOWNLOAD_URL: &str = "https://download-new.ets2mp.com/files/";

async fn get_content_files(client: &reqwest::Client) -> Result<ContentFiles> {
    let raw_content_files: RawContentFiles = client.get(UPDATE_URL).send().await?.json().await?;

    Ok(ContentFiles::from(raw_content_files))
}

async fn update_files(
    client: &reqwest::Client,
    content_files: ContentFiles,
    game: &Game,
    content_dir: PathBuf,
) -> Result<()> {
    let mut files = content_files.shared.clone();
    match game {
        Game::ETS2 => files.extend(content_files.ets2),
        Game::ATS => files.extend(content_files.ats),
    }

    println!("updating files for game: {:?}", game);

    if !content_dir.exists() {
        tokio::fs::create_dir_all(&content_dir).await?;
        verify_and_download(&client, &files, content_dir, true).await
    } else {
        verify_and_download(&client, &files, content_dir, false).await
    }

    // check if content dir exists -> download first -> check md5 of files -> redownload if needed
}

static PROGRESS_BAR_TEMPLATE: Lazy<ProgressStyle> = Lazy::new(|| {
    indicatif::ProgressStyle::with_template(
        "{spinner:.green} {msg} [{wide_bar:.cyan/blue}] {percent}% {eta}",
    )
    .unwrap()
    .with_key("eta", |state: &ProgressState, w: &mut dyn Write| {
        write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap()
    })
    .progress_chars("#>-")
});

async fn download_files(
    client: &reqwest::Client,
    content_files: &Vec<ContentFile>,
    content_dir: &PathBuf,
) -> Result<()> {
    use futures_util::StreamExt as _;
    let concurrency = Arc::new(Semaphore::new(8)); // todo: clap config
    let progress_bars = MultiProgress::new();
    // let progress_bar_style = indicatif::ProgressStyle::with_template(
    //     "{msg} {bar:40.cyan/blue} {percent}% [{elapsed_precise}]",
    // )
    // .unwrap(); // tod o: could just be compile time or static
    let content_dir = content_dir
        .canonicalize()
        .expect("failed horribly to canonicalize content dir");
    // println!("path: {:?}", content_dir);

    let handles = content_files.into_iter().map(|file| {
        let progress_bar = progress_bars.clone();
        let progress_bar_style = PROGRESS_BAR_TEMPLATE.clone();
        let client = client.clone();
        let concurrency = concurrency.clone();
        let url = format!("{DOWNLOAD_URL}{}", file.file_path);
        let path = content_dir.join(&file.file_path);
        // println!("path: {:?}", path);

        tokio::spawn(async move {
            let _ticket = concurrency.acquire().await?;

            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }

            let resp = client.get(url).send().await?;
            let content_size = resp.content_length().unwrap_or(0);

            let progress_bar = progress_bar.add(ProgressBar::new(content_size));
            progress_bar.set_style(progress_bar_style);
            progress_bar.set_message(format!(
                "{:#?}",
                path.file_name().unwrap_or_else(|| OsStr::new("unknown"))
            ));

            let mut file = tokio::fs::File::create(&path).await?;
            let mut stream = resp.bytes_stream();

            while let Some(chunk_result) = stream.next().await {
                let chunk = chunk_result?;
                // println!("downloading chunk of size: {}", chunk.len());
                file.write_all(&chunk).await?;
                progress_bar.inc(chunk.len() as u64);
            }

            file.flush().await?;
            progress_bar.finish_and_clear();

            // println!("downloaded {} bytes", downloaded);

            // println!("are we stuck?");
            Ok::<(), Error>(())
        })
    });

    for handle in handles {
        handle.await??;
    }

    println!("seems done");
    Ok(())
}

// gotta love working on an async environment.
// the need of having everything touching async be async or else we would block everything like a brick wall.
async fn check_file_hash(content_file: &ContentFile, file_path: &PathBuf) -> Result<bool> {
    if !file_path.exists() {
        return Ok(false);
    }

    let file = tokio::fs::File::open(file_path).await?;
    let mut reader = BufReader::new(file);
    let mut context = md5::Context::new();
    let mut buffer = [0; 8192];

    loop {
        let n = reader.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        context.consume(&buffer[..n]);
    }

    let hash = context.finalize();
    let hash = format!("{:x}", hash);
    if hash == content_file.md5 {
        return Ok(true);
    }

    Ok(false)
}

async fn verify_and_download(
    client: &reqwest::Client,
    content_files: &Vec<ContentFile>,
    content_dir: PathBuf,
    download_first: bool,
) -> Result<()> {
    let concurrency = Arc::new(Semaphore::new(8)); // todo: clap config
    let progress_bar = ProgressBar::hidden();
    progress_bar.set_style(PROGRESS_BAR_TEMPLATE.clone());
    progress_bar.set_message("Verifying");
    progress_bar.set_length((content_files.len() + 1) as u64);
    progress_bar.set_position(0);

    let content_dir = content_dir
        .canonicalize()
        .expect("failed horribly to canonicalize content dir");

    if download_first {
        download_files(client, content_files, &content_dir).await?;
    }

    progress_bar.set_draw_target(ProgressDrawTarget::stderr());
    loop {
        let failed_files = Arc::new(tokio::sync::Mutex::new(Vec::new()));

        let handles = content_files.into_iter().map(|file| {
            let progress_bar = progress_bar.clone();
            let failed_files = failed_files.clone();
            let concurrency = concurrency.clone();
            let file = file.clone();
            let path = content_dir.join(&file.file_path);
            tokio::spawn(async move {
                let _ticket = concurrency.acquire().await?;
                let mut failed_files = failed_files.lock().await;

                // println!("verifying");

                if !path.exists() || !check_file_hash(&file, &path).await? {
                    failed_files.push(file.clone());
                }

                progress_bar.inc(1);
                Ok::<(), Error>(())
            })
        });
        // for (idx, file) in content_files.iter().enumerate() {}
        for handle in handles {
            handle.await??;
        }
        progress_bar.finish_and_clear();

        let mut failed_files = failed_files.lock().await;

        if !failed_files.is_empty() {
            println!(
                "Failed to verify {} files. Retrying download...",
                failed_files.len()
            );
            download_files(client, &failed_files, &content_dir).await?;
            failed_files.clear();
        } else {
            break;
        }
    }
    println!("finished verifying");

    Ok(())
}
