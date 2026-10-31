use crate::manager::profile::Profile;

#[cfg(target_os = "windows")]
const PROFILE_ARGUMENT_LIMIT: usize = 4096;

pub struct BackgroundController {
    #[cfg(target_os = "windows")]
    child: Option<std::process::Child>,
    profile: Profile,
}

impl BackgroundController {
    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    pub fn connect_existing() -> Result<Option<Self>, String> {
        #[cfg(target_os = "windows")]
        {
            return windows::connect_existing();
        }

        #[cfg(not(target_os = "windows"))]
        Ok(None)
    }

    pub fn launch(profile: &Profile) -> Result<Self, String> {
        #[cfg(target_os = "windows")]
        {
            return windows::launch(profile);
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = profile;
            Err("Applied background effects are currently supported on Windows only.".to_string())
        }
    }

    pub fn set_profile(&mut self, profile: &Profile) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        {
            windows::set_profile(profile)?;
            self.profile = windows::without_profile_name(profile);
            return Ok(());
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = profile;
            Err("Applied background effects are currently supported on Windows only.".to_string())
        }
    }

    pub fn stop(&mut self) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        {
            return windows::stop(&mut self.child);
        }

        #[cfg(not(target_os = "windows"))]
        Ok(())
    }
}

pub fn run(profile: Profile) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        return windows::run(profile);
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = profile;
        Err("Applied background effects are currently supported on Windows only.".to_string())
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use std::{
        ffi::OsStr,
        fs::File,
        io::{self, Read, Write},
        os::windows::{
            ffi::OsStrExt,
            io::{FromRawHandle, RawHandle},
            process::CommandExt,
        },
        process::{Child, Command},
        ptr, thread,
        time::{Duration, Instant},
    };

    use serde::{Deserialize, Serialize};
    use winapi::{
        shared::{
            minwindef::DWORD,
            winerror::{ERROR_FILE_NOT_FOUND, ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED},
        },
        um::{
            errhandlingapi::GetLastError,
            fileapi::{CreateFileW, OPEN_EXISTING},
            handleapi::INVALID_HANDLE_VALUE,
            namedpipeapi::{ConnectNamedPipe, CreateNamedPipeW, SetNamedPipeHandleState, WaitNamedPipeW},
            winbase::{
                CREATE_BREAKAWAY_FROM_JOB, CREATE_NO_WINDOW, FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX, PIPE_NOWAIT, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
            },
            winnt::{GENERIC_READ, GENERIC_WRITE},
        },
    };

    use crate::manager::{profile::Profile, EffectManager, OperationMode};

    use super::PROFILE_ARGUMENT_LIMIT;

    const PIPE_NAME: &str = r"\\.\pipe\LegionKeyboardRgb.Apply.v1";
    const MAX_FRAME_SIZE: usize = 4096;
    const PIPE_TIMEOUT: Duration = Duration::from_secs(3);

    #[derive(Serialize, Deserialize)]
    enum CommandMessage {
        Ping,
        SetProfile(Profile),
        Stop,
    }

    #[derive(Serialize, Deserialize)]
    struct ResponseMessage {
        success: bool,
        error: Option<String>,
        profile: Option<Profile>,
    }

    pub fn connect_existing() -> Result<Option<super::BackgroundController>, String> {
        match transact(&CommandMessage::Ping, Duration::from_millis(500))? {
            Some(response) => {
                let profile = response.profile.ok_or_else(|| "The background effect process did not provide its active profile.".to_string())?;
                Ok(Some(super::BackgroundController { child: None, profile }))
            }
            None => Ok(None),
        }
    }

    pub fn launch(profile: &Profile) -> Result<super::BackgroundController, String> {
        let profile = without_profile_name(profile);
        let serialized_profile = serde_json::to_string(&profile).map_err(|error| format!("Could not encode the applied effect: {error}"))?;
        if serialized_profile.len() > PROFILE_ARGUMENT_LIMIT {
            return Err("The selected effect profile is too large to start in the background.".to_string());
        }

        let executable = std::env::current_exe().map_err(|error| format!("Could not locate the application executable: {error}"))?;
        let mut child = Command::new(executable)
            .arg("--background-effect")
            .arg(serialized_profile)
            .creation_flags(CREATE_NO_WINDOW | CREATE_BREAKAWAY_FROM_JOB)
            .spawn()
            .map_err(|error| format!("Could not start the background effect process: {error}"))?;

        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return Err(format!("The background effect process exited during startup with status {status}.")),
                Ok(None) => {}
                Err(error) => {
                    let terminate_error = terminate_child(&mut child).err();
                    return Err(match terminate_error {
                        Some(terminate_error) => format!("Could not inspect the background effect process: {error}; cleanup failed: {terminate_error}"),
                        None => format!("Could not inspect the background effect process: {error}"),
                    });
                }
            }

            match transact(&CommandMessage::Ping, Duration::from_millis(250)) {
                Ok(Some(_)) => return Ok(super::BackgroundController { child: Some(child), profile }),
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(40)),
                Ok(None) => {
                    terminate_child(&mut child)?;
                    return Err("The background effect process did not become ready in time.".to_string());
                }
                Err(error) => {
                    terminate_child(&mut child)?;
                    return Err(format!("Could not connect to the background effect process: {error}"));
                }
            }
        }
    }

    pub fn set_profile(profile: &Profile) -> Result<(), String> {
        transact(&CommandMessage::SetProfile(without_profile_name(profile)), PIPE_TIMEOUT)?
            .map(|_| ())
            .ok_or_else(|| "The background effect process did not respond.".to_string())
    }

    pub fn stop(child: &mut Option<Child>) -> Result<(), String> {
        transact(&CommandMessage::Stop, PIPE_TIMEOUT)?.ok_or_else(|| "The background effect process did not respond to the stop request.".to_string())?;

        if let Some(mut child) = child.take() {
            let deadline = Instant::now() + PIPE_TIMEOUT;
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => return Ok(()),
                    Ok(None) => {}
                    Err(error) => {
                        let terminate_error = terminate_child(&mut child).err();
                        return Err(match terminate_error {
                            Some(terminate_error) => format!("Could not inspect the stopped background process: {error}; cleanup failed: {terminate_error}"),
                            None => format!("Could not inspect the stopped background process: {error}"),
                        });
                    }
                }
                if Instant::now() >= deadline {
                    terminate_child(&mut child)?;
                    return Err("The background effect process did not exit after stopping.".to_string());
                }
                thread::sleep(Duration::from_millis(20));
            }
        }
        Ok(())
    }

    pub fn run(profile: Profile) -> Result<(), String> {
        let mut manager = EffectManager::new(OperationMode::Gui).map_err(|error| format!("Could not initialize the background keyboard manager: {error}"))?;
        manager.set_profile(profile.clone());
        serve(manager, profile)
    }

    fn serve(mut manager: EffectManager, mut profile: Profile) -> Result<(), String> {
        let pipe_name: Vec<u16> = OsStr::new(PIPE_NAME).encode_wide().chain(Some(0)).collect();
        let mut first_instance = true;

        loop {
            let open_mode = PIPE_ACCESS_DUPLEX | if first_instance { FILE_FLAG_FIRST_PIPE_INSTANCE } else { 0 };
            let handle = unsafe {
                CreateNamedPipeW(
                    pipe_name.as_ptr(),
                    open_mode,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                    1,
                    MAX_FRAME_SIZE as DWORD,
                    MAX_FRAME_SIZE as DWORD,
                    0,
                    ptr::null_mut(),
                )
            };
            if handle == INVALID_HANDLE_VALUE {
                let error = io::Error::last_os_error();
                manager.shutdown();
                return Err(format!("Could not create the local background-effect control pipe: {error}"));
            }
            first_instance = false;

            let mut stream = unsafe { File::from_raw_handle(handle as RawHandle) };
            let connected = unsafe { ConnectNamedPipe(handle, ptr::null_mut()) };
            if connected == 0 && unsafe { GetLastError() } != ERROR_PIPE_CONNECTED {
                let error = io::Error::last_os_error();
                eprintln!("Background effect pipe connection failed: {error}");
                continue;
            }

            let mut mode = PIPE_READMODE_BYTE | PIPE_NOWAIT;
            if unsafe { SetNamedPipeHandleState(handle, &mut mode, ptr::null_mut(), ptr::null_mut()) } == 0 {
                eprintln!("Could not configure a bounded background effect pipe read: {}", io::Error::last_os_error());
                continue;
            }

            let command = match read_frame::<CommandMessage>(&mut stream) {
                Ok(command) => command,
                Err(error) => {
                    let response = ResponseMessage {
                        success: false,
                        error: Some(error),
                        profile: None,
                    };
                    if let Err(write_error) = write_frame(&mut stream, &response) {
                        eprintln!("Could not respond to an invalid background request: {write_error}");
                    }
                    continue;
                }
            };

            match command {
                CommandMessage::Ping => {
                    if let Err(error) = write_response(&mut stream, ResponseMessage::success(Some(&profile))) {
                        eprintln!("Could not acknowledge the background effect process: {error}");
                    }
                }
                CommandMessage::SetProfile(new_profile) => {
                    manager.set_profile(new_profile.clone());
                    profile = new_profile;
                    if let Err(error) = write_response(&mut stream, ResponseMessage::success(None)) {
                        eprintln!("Could not acknowledge the background effect update: {error}");
                    }
                }
                CommandMessage::Stop => {
                    manager.shutdown();
                    write_response(&mut stream, ResponseMessage::success(None))?;
                    return Ok(());
                }
            }
        }
    }

    fn transact(command: &CommandMessage, timeout: Duration) -> Result<Option<ResponseMessage>, String> {
        let mut stream = match open_pipe(timeout)? {
            Some(stream) => stream,
            None => return Ok(None),
        };
        write_frame(&mut stream, command)?;
        let response: ResponseMessage = read_frame(&mut stream)?;
        if response.success {
            Ok(Some(response))
        } else {
            Err(response.error.unwrap_or_else(|| "The background effect process rejected the request.".to_string()))
        }
    }

    fn open_pipe(timeout: Duration) -> Result<Option<File>, String> {
        let pipe_name: Vec<u16> = OsStr::new(PIPE_NAME).encode_wide().chain(Some(0)).collect();
        let deadline = Instant::now() + timeout;

        loop {
            let handle = unsafe { CreateFileW(pipe_name.as_ptr(), GENERIC_READ | GENERIC_WRITE, 0, ptr::null_mut(), OPEN_EXISTING, 0, ptr::null_mut()) };
            if handle != INVALID_HANDLE_VALUE {
                return Ok(Some(unsafe { File::from_raw_handle(handle as RawHandle) }));
            }

            let error_code = unsafe { GetLastError() };
            if error_code != ERROR_FILE_NOT_FOUND && error_code != ERROR_PIPE_BUSY {
                return Err(format!("Could not open the background effect control pipe: {}", io::Error::last_os_error()));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }

            if error_code == ERROR_PIPE_BUSY {
                unsafe {
                    WaitNamedPipeW(pipe_name.as_ptr(), 100);
                }
            } else {
                thread::sleep(Duration::from_millis(20));
            }
        }
    }

    fn read_frame<T: for<'de> Deserialize<'de>>(stream: &mut File) -> Result<T, String> {
        let mut length_bytes = [0_u8; 4];
        read_exact_timeout(stream, &mut length_bytes, Duration::from_secs(2))?;
        let length = u32::from_le_bytes(length_bytes) as usize;
        if length == 0 || length > MAX_FRAME_SIZE {
            return Err(format!("Invalid background control message size: {length} bytes."));
        }

        let mut payload = vec![0_u8; length];
        read_exact_timeout(stream, &mut payload, Duration::from_secs(2))?;
        serde_json::from_slice(&payload).map_err(|error| format!("Invalid background control message: {error}"))
    }

    fn write_frame<T: Serialize>(stream: &mut File, message: &T) -> Result<(), String> {
        let payload = serde_json::to_vec(message).map_err(|error| format!("Could not encode a background control message: {error}"))?;
        if payload.is_empty() || payload.len() > MAX_FRAME_SIZE {
            return Err("Background control message exceeds the allowed size.".to_string());
        }

        stream
            .write_all(&(payload.len() as u32).to_le_bytes())
            .map_err(|error| format!("Could not write background control message: {error}"))?;
        stream.write_all(&payload).map_err(|error| format!("Could not write background control message: {error}"))
    }

    fn read_exact_timeout(stream: &mut File, mut buffer: &mut [u8], timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        while !buffer.is_empty() {
            match stream.read(buffer) {
                Ok(0) => return Err("The background control pipe closed before the full message arrived.".to_string()),
                Ok(read) => buffer = &mut buffer[read..],
                Err(error) if error.raw_os_error() == Some(ERROR_NO_DATA as i32) => {
                    if Instant::now() >= deadline {
                        return Err("Timed out while reading a background control message.".to_string());
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(format!("Could not read background control message: {error}")),
            }
        }
        Ok(())
    }

    fn write_response(stream: &mut File, response: ResponseMessage) -> Result<(), String> {
        write_frame(stream, &response)
    }

    impl ResponseMessage {
        fn success(profile: Option<&Profile>) -> Self {
            Self {
                success: true,
                error: None,
                profile: profile.cloned(),
            }
        }
    }

    pub fn without_profile_name(profile: &Profile) -> Profile {
        let mut profile = profile.clone();
        profile.name = None;
        profile
    }

    fn terminate_child(child: &mut Child) -> Result<(), String> {
        if child.try_wait().map_err(|error| format!("Could not inspect the failed background effect process: {error}"))?.is_none() {
            child.kill().map_err(|error| format!("Could not stop the failed background effect process: {error}"))?;
            child.wait().map_err(|error| format!("Could not reap the failed background effect process: {error}"))?;
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::{without_profile_name, CommandMessage, MAX_FRAME_SIZE};
        use crate::manager::profile::Profile;

        #[test]
        fn background_profile_commands_are_bounded_and_omit_names() {
            let profile = Profile {
                name: Some("Private profile label".to_string()),
                ..Profile::default()
            };
            let sanitized = without_profile_name(&profile);
            assert!(sanitized.name.is_none());

            let encoded = serde_json::to_vec(&CommandMessage::SetProfile(sanitized.clone())).unwrap();
            assert!(encoded.len() <= MAX_FRAME_SIZE);
            match serde_json::from_slice::<CommandMessage>(&encoded).unwrap() {
                CommandMessage::SetProfile(decoded) => assert_eq!(decoded, sanitized),
                _ => panic!("A profile command must round-trip as a profile update."),
            }
        }
    }
}
