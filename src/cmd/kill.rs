use sysinfo::{ProcessRefreshKind, RefreshKind, System, UpdateKind};

use crate::{
    STEAMWORKS_CLIENT,
    cmd::{Kill, Run},
    errors::TResult,
    game::{get_available_game, get_game_path, get_specific_game},
};

impl Run for Kill {
    async fn run(&self) -> TResult<()> {
        let game = if let Some(game) = self.game {
            get_specific_game(&STEAMWORKS_CLIENT, game)
        } else {
            get_available_game(&STEAMWORKS_CLIENT)
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

        let game_path = get_game_path(&STEAMWORKS_CLIENT, game)?;
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
