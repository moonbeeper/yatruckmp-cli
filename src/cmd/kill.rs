use sysinfo::{ProcessRefreshKind, RefreshKind, System, UpdateKind};

use crate::{
    cmd::{Kill, Run},
    errors::TResult,
    game::{get_available_game, get_game_path, get_specific_game, get_steamworks_client},
};

impl Run for Kill {
    async fn run(&self) -> TResult<()> {
        let steamworks = get_steamworks_client()?;

        let game = if let Some(game) = self.game {
            get_specific_game(&steamworks, game)
        } else {
            get_available_game(&steamworks)
        }?;

        let sysinfo = System::new_with_specifics(
            RefreshKind::nothing()
                .with_processes(ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet)),
        );
        // sysinfo.refresh_processes_specifics(
        //     ProcessesToUpdate::All,
        //     true,
        //     ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
        // );

        let game_path = get_game_path(&steamworks, game)?;
        println!("game_path: {:?}", game_path);

        for (_, process) in sysinfo.processes() {
            if process.exe() == Some(&game_path) {
                println!("Found game process! Killing it... :3");
                match process.kill() {
                    true => println!("Game process killed successfully"),
                    false => println!("Brutally failed to kill the game process"),
                }
                break;
            }
        }

        Ok(())
    }
}
