use crate::{
    cmd::{GameCmd, GameCommand, Run},
    errors::TResult,
};

pub mod server;

impl Run for GameCmd {
    async fn run(&self) -> TResult<()> {
        match &self.cmd {
            Some(cmd) => match cmd {
                GameCommand::Servers(cmd) => cmd.run().await?,
            },
            None => {}
        }
        Ok(())
    }
}
