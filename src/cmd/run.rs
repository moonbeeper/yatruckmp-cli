use std::{os::windows::ffi::OsStrExt as _, path::PathBuf};

use clap::crate_name;
use dirs::data_dir;
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

use crate::{
    cmd::{Run, RunGame, Update},
    errors::{Error, TResult},
    game::{get_available_game, get_game_path, get_specific_game, get_steamworks_client},
};

impl Run for RunGame {
    async fn run(&self) -> crate::errors::TResult<()> {
        let steamworks = get_steamworks_client()?;

        let game = if let Some(game) = self.game {
            get_specific_game(&steamworks, game)
        } else {
            get_available_game(&steamworks)
        }?;

        let game_path = get_game_path(&steamworks, game)?;
        let path: Vec<u16> = game_path.as_os_str().encode_wide().chain(Some(0)).collect(); // uft16

        let content_dir = data_dir()
            .ok_or_else(|| Error::NoAppdataPath)?
            .join(crate_name!())
            .join("content")
            .to_path_buf(); // TODO: make this configurable
        let dll_path = content_dir.join(game.dll());

        if !self.no_verify {
            Update {
                game: Some(game),
                ..Update::default()
            }
            .run()
            .await?;
        }

        if !dll_path.exists() {
            return Err(Error::FailedInjectingDLL(
                "failed to find the mod's dll file, you might want to update the mod files".into(),
            ));
        }

        println!("Launching {:?}!", game);
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
}

// your typical remote thread dll or shellcode injection lol
fn inject_dll(process: HANDLE, dll_path: PathBuf) -> TResult<()> {
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
