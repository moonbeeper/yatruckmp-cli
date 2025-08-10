use clap::crate_version;

use crate::{
    cmd::{ModVersion, Run},
    errors::TResult,
};

impl Run for ModVersion {
    async fn run(&self) -> TResult<()> {
        let reqwest_client = reqwest::Client::builder()
            .user_agent(format!("Yet Another TruckersMP Cli/{:?}", crate_version!()))
            .build()?;

        let game_info = get_game_info(&reqwest_client).await?;

        println!("CLI version: {}", crate_version!());
        println!("Mod version: {}", game_info.version);
        println!("Mod stage: {}", game_info.stage);
        println!(
            "Supported ETS2 game version: {}",
            game_info.supported_ets2_version
        );
        println!(
            "Supported ATS game version: {}",
            game_info.supported_ats_version
        );

        Ok(())
    }
}

#[derive(Debug, serde::Deserialize)]
struct GameInformation {
    #[serde(rename = "name")]
    version: String,
    // numeric: String,
    stage: String,
    // ets2mp_checksum: RawGameChecksum,
    // atsmp_checksum: RawGameChecksum,
    // time: String,
    #[serde(rename = "supported_game_version")]
    supported_ets2_version: String,
    #[serde(rename = "supported_ats_game_version")]
    supported_ats_version: String,
}

// #[derive(Debug, serde::Deserialize)]
// struct RawGameChecksum {
//     dll: String,
//     adb: String,
// }

async fn get_game_info(client: &reqwest::Client) -> TResult<GameInformation> {
    let result: GameInformation = client
        .get("https://api.truckersmp.com/v2/version")
        .send()
        .await?
        .json()
        .await?;

    Ok(result)
}
