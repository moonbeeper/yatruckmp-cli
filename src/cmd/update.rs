use std::{ffi::OsStr, fmt::Write, path::PathBuf, sync::Arc};

use clap::{crate_name, crate_version};
use dirs::data_dir;
use futures_util::StreamExt as _;
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressState, ProgressStyle};
use once_cell::sync::Lazy;
use tokio::{
    fs,
    io::{AsyncReadExt as _, AsyncWriteExt as _, BufReader},
    sync::Semaphore,
};

use crate::{
    STEAMWORKS_CLIENT,
    cmd::{Run, Update},
    errors::{Error, TResult},
    game::{Game, get_available_game, get_specific_game},
};

const UPDATE_URL: &str = "https://update.ets2mp.com/files.json";
const DOWNLOAD_URL: &str = "https://download-new.ets2mp.com/files/";

static PROGRESS_BAR_TEMPLATE: Lazy<ProgressStyle> = Lazy::new(|| {
    ProgressStyle::with_template("{spinner:.green} {msg} [{wide_bar:.cyan/blue}] {percent}% {eta}")
        .unwrap()
        .with_key("eta", |state: &ProgressState, w: &mut dyn Write| {
            write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap()
        })
        .progress_chars("#>-")
});

impl Run for Update {
    async fn run(&self) -> crate::errors::TResult<()> {
        println!("updating TruckersMP mod files");

        let reqwest_client = reqwest::Client::builder()
            .user_agent(format!("Yet Another TruckersMP Cli/{:?}", crate_version!()))
            .build()?;
        let content_files = get_content_files(&reqwest_client).await?;

        let game = if let Some(game) = self.game {
            get_specific_game(&STEAMWORKS_CLIENT, game)
        } else {
            get_available_game(&STEAMWORKS_CLIENT)
        }?;

        let content_dir = data_dir()
            .ok_or_else(|| Error::NoAppdataPath)?
            .join(crate_name!())
            .join("content")
            .to_path_buf(); // TODO: make this configurable

        // I could get the parent folder but that opens the risk of me accidentally deleting the whole system32 folder lol.
        if self.clean && content_dir.exists() {
            fs::remove_dir_all(&content_dir).await?;
            fs::create_dir_all(&content_dir).await?;
        }

        update_files(
            &reqwest_client,
            content_files,
            &game,
            content_dir,
            self.clean,
        )
        .await?;

        Ok(())
    }
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

#[derive(serde::Deserialize, Debug, Clone)]
struct RawContentFiles {
    #[serde(rename = "Files")]
    files: Vec<RawContentFile>,
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

#[derive(serde::Deserialize, Debug, PartialEq, Eq, Clone, Copy)]
enum RawContentType {
    #[serde(rename = "ets2")]
    ETS2,
    #[serde(rename = "ats")]
    ATS,
    #[serde(rename = "system")]
    System,
}

async fn get_content_files(client: &reqwest::Client) -> TResult<ContentFiles> {
    let raw_content_files: RawContentFiles = client.get(UPDATE_URL).send().await?.json().await?;

    Ok(ContentFiles::from(raw_content_files))
}

async fn update_files(
    client: &reqwest::Client,
    content_files: ContentFiles,
    game: &Game,
    content_dir: PathBuf,
    clean: bool,
) -> TResult<()> {
    let mut files = content_files.shared.clone();
    match game {
        Game::ETS2 => files.extend(content_files.ets2),
        Game::ATS => files.extend(content_files.ats),
    }

    println!("updating files for game: {:?}", game);

    if !content_dir.exists() || clean {
        tokio::fs::create_dir_all(&content_dir).await?;
        verify_and_download(&client, &files, content_dir, true).await
    } else {
        verify_and_download(&client, &files, content_dir, false).await
    }
}

async fn download_files(
    client: &reqwest::Client,
    content_files: &Vec<ContentFile>,
    content_dir: &PathBuf,
) -> TResult<()> {
    let concurrency = Arc::new(Semaphore::new(8)); // todo: clap config
    let progress_bars = MultiProgress::new();

    let main_pb = progress_bars.add(ProgressBar::new(content_files.len() as u64));
    main_pb.set_style(PROGRESS_BAR_TEMPLATE.clone());
    main_pb.tick();

    let content_dir = content_dir
        .canonicalize()
        .expect("failed horribly to canonicalize content dir");

    let handles = content_files.into_iter().map(|file| {
        let progress_bar = progress_bars.clone();
        let progress_bar_style = PROGRESS_BAR_TEMPLATE.clone();
        let client = client.clone();
        let concurrency = concurrency.clone();
        let url = format!("{DOWNLOAD_URL}{}", file.file_path);
        let path = content_dir.join(&file.file_path);
        let main_pb = main_pb.clone();

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
                file.write_all(&chunk).await?;
                progress_bar.inc(chunk.len() as u64);
            }

            file.flush().await?;
            progress_bar.finish_and_clear();

            main_pb.inc(1);
            Ok::<(), Error>(())
        })
    });

    for handle in handles {
        handle.await??;
    }

    Ok(())
}

// gotta love working on an async environment.
// the need of having everything touching async be async or else we would block everything like a brick wall.
async fn check_file_hash(content_file: &ContentFile, file_path: &PathBuf) -> TResult<bool> {
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
) -> TResult<()> {
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

                if !path.exists() || !check_file_hash(&file, &path).await? {
                    failed_files.push(file.clone());
                }

                progress_bar.inc(1);
                Ok::<(), Error>(())
            })
        });

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

    println!("Finished verifying files");
    Ok(())
}
