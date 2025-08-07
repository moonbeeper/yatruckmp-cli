use std::{
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use steamworks::{AppId, Client};
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

#[derive(PartialEq, Eq, Clone, Copy)]
enum Game {
    ETS2 = 227300,
    ATS = 270880,
    None = 0,
}

impl From<Game> for AppId {
    fn from(value: Game) -> Self {
        AppId(value as u32)
    }
}

fn get_game(client: &Client) -> Game {
    if client.apps().is_subscribed_app(Game::ETS2.into()) {
        if !client.apps().is_app_installed(Game::ETS2.into()) {
            panic!("ETS2 is not installed");
        }

        return Game::ETS2;
    } else if client.apps().is_subscribed_app(Game::ATS.into()) {
        if !client.apps().is_app_installed(Game::ATS.into()) {
            panic!("ATS is not installed");
        }

        return Game::ATS;
    }

    return Game::None;
}

fn main() {
    let client = Client::init_app(AppId(480)).unwrap(); // app id 480 is the safe bet as its the sdk demo app

    let game = get_game(&client);
    // should be maybe a result instead.
    if game == Game::None {
        panic!("nope");
    }

    let game_dir = client.apps().app_install_dir(game.into());
    println!("game_dir: {:?}", game_dir);

    let path = Path::new(&game_dir)
        .join("bin")
        .join("win_x64")
        .join("eurotrucks2.exe");
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect(); // uft16

    let dll_path = Path::new("C:\\Users\\toast\\AppData\\Roaming\\TruckersMP\\installation")
        .join("core_ets2mp.dll");

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
        .unwrap();

        inject_dll(process_info.hProcess, dll_path);
        ResumeThread(process_info.hThread);
        CloseHandle(process_info.hThread).unwrap();
    }
}

// your typical remote thread dll or shellcode injection lol
fn inject_dll(process: HANDLE, dll_path: PathBuf) {
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
            CloseHandle(process).unwrap();
            panic!("failed to allocate memory in the game process");
        }

        WriteProcessMemory(
            process,
            alloc_addr,
            dll_path.as_ptr() as *const _,
            dll_path_len,
            None,
        )
        .unwrap_or_else(|_| {
            CloseHandle(process).unwrap();
            panic!("failed to write to game process memory");
        });

        let kernel_handle = GetModuleHandleA(s!("kernel32.dll")).unwrap_or_else(|_| {
            CloseHandle(process).unwrap();
            panic!("failed get kernel handle");
        });
        let load_library_addr =
            GetProcAddress(kernel_handle, s!("LoadLibraryW")).unwrap_or_else(|| {
                CloseHandle(process).unwrap();
                panic!("failed to get LoadLibraryW addr");
            });

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
        .unwrap_or_else(|_| {
            CloseHandle(process).unwrap();
            panic!("failed to create remote thread");
        });

        WaitForSingleObject(remote_thread, INFINITE);
        CloseHandle(process).unwrap();
    }
}
