use std::{os::windows::ffi::OsStrExt as _, path::PathBuf};

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
    STEAMWORKS_CLIENT,
    cmd::{Run, RunGame, Update},
    errors::{Error, TResult},
    game::{get_available_game, get_game_path, get_specific_game},
};

impl Run for RunGame {
    async fn run(&self) -> crate::errors::TResult<()> {
        let game = if let Some(game) = self.game {
            get_specific_game(&STEAMWORKS_CLIENT, game)
        } else {
            get_available_game(&STEAMWORKS_CLIENT)
        }?;

        let game_path = get_game_path(&STEAMWORKS_CLIENT, game)?;
        let path: Vec<u16> = game_path.as_os_str().encode_wide().chain(Some(0)).collect(); // uft16

        let current_exe_path = std::env::current_exe()?;
        let current_exe_path = current_exe_path
            .parent()
            .expect("Executable must be in some directory")
            .canonicalize()?;

        let content_dir = current_exe_path.join("content").to_path_buf(); // TODO: make this configurable
        let dll_path = content_dir.join(game.dll());

        // TODO: I shouldn't be doing this here.
        Update {
            clean: false,
            game: Some(game),
        }
        .run()
        .await?;

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
