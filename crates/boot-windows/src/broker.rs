use boot_core::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "action",
    content = "payload",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum BrokerRequest {
    Refresh,
    Prepare(PlanRequest),
    Apply { plan_id: Uuid },
    ListBackups,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum BrokerResponse {
    Snapshot(Box<Snapshot>),
    ChangePlan(ChangePlan),
    ApplyReport(ApplyReport),
    Backups(Vec<BackupRecord>),
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    schema_version: u32,
    nonce: String,
    request: BrokerRequest,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    schema_version: u32,
    nonce: String,
    result: BootResult<BrokerResponse>,
}
pub fn decode_request(bytes: &[u8], nonce: &str) -> BootResult<BrokerRequest> {
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(crate::error(ErrorCode::InvalidMessage, "Nachricht zu groß"));
    }
    let envelope: Envelope = serde_json::from_slice(bytes)
        .map_err(|_| crate::error(ErrorCode::InvalidMessage, "Unzulässige Protokollnachricht"))?;
    if envelope.schema_version != SCHEMA_VERSION || envelope.nonce != nonce {
        return Err(crate::error(
            ErrorCode::UntrustedPeer,
            "Protokollversion oder Nonce stimmt nicht überein",
        ));
    }
    Ok(envelope.request)
}
#[derive(Debug, Clone)]
pub struct PeerIdentity {
    pub pid: u32,
    pub session: u32,
    pub sid: String,
    pub image_directory: String,
}
pub fn verify_peer(expected: &PeerIdentity, actual: &PeerIdentity) -> BootResult<()> {
    if expected.pid != actual.pid
        || expected.session != actual.session
        || expected.sid != actual.sid
        || !expected
            .image_directory
            .eq_ignore_ascii_case(&actual.image_directory)
    {
        return Err(crate::error(
            ErrorCode::UntrustedPeer,
            "Pipe-Gegenstelle ist nicht der erwartete Prozess",
        ));
    }
    Ok(())
}
pub trait BrokerTransport: Send + Sync {
    fn exchange(&self, request: BrokerRequest) -> BootResult<BrokerResponse>;
}
pub struct BrokerClient {
    transport: Box<dyn BrokerTransport>,
}
impl Default for BrokerClient {
    fn default() -> Self {
        Self::new()
    }
}
impl BrokerClient {
    pub fn new() -> Self {
        Self::with_transport(NativeTransport)
    }
    pub fn with_transport(transport: impl BrokerTransport + 'static) -> Self {
        Self {
            transport: Box::new(transport),
        }
    }
    /// Blocking boundary, including UAC consent. GUI callers must use their blocking worker.
    pub fn request(&self, request: BrokerRequest) -> BootResult<BrokerResponse> {
        self.transport.exchange(request)
    }
}
struct NativeTransport;
impl BrokerTransport for NativeTransport {
    fn exchange(&self, request: BrokerRequest) -> BootResult<BrokerResponse> {
        #[cfg(windows)]
        {
            windows_pipe::exchange(request)
        }
        #[cfg(not(windows))]
        {
            let _ = request;
            Err(crate::error(
                ErrorCode::NotImplemented,
                "Windows erforderlich",
            ))
        }
    }
}
pub fn run_helper() -> BootResult<()> {
    #[cfg(windows)]
    {
        windows_pipe::helper()
    }
    #[cfg(not(windows))]
    {
        Err(crate::error(
            ErrorCode::NotImplemented,
            "Windows erforderlich",
        ))
    }
}
/// Frontend requests cannot select a target path, method name, shell command or owner SID.
pub fn dispatch(request: BrokerRequest, caller_sid: &str) -> BootResult<BrokerResponse> {
    match request {
        BrokerRequest::Refresh => {
            crate::collect_snapshot().map(|s| BrokerResponse::Snapshot(Box::new(s)))
        }
        BrokerRequest::ListBackups => crate::store::ProtectedStore::open(caller_sid)?
            .list_backups()
            .map(BrokerResponse::Backups),
        BrokerRequest::Prepare(_) | BrokerRequest::Apply { .. } => Err(crate::error(
            ErrorCode::NotImplemented,
            "Planen und Anwenden sind bis zum Transaktionsdienst gesperrt",
        )),
    }
}
#[cfg(windows)]
mod windows_pipe {
    use super::*;
    use crate::native::*;
    use std::{
        mem::size_of,
        path::PathBuf,
        time::{Duration, Instant},
    };
    use windows::{
        Win32::{
            Foundation::*,
            Storage::FileSystem::*,
            System::{IO::*, Pipes::*, RemoteDesktop::ProcessIdToSessionId, Threading::*},
            UI::{Shell::*, WindowsAndMessaging::SW_HIDE},
        },
        core::{PCWSTR, PWSTR},
    };
    const TIMEOUT: Duration = Duration::from_secs(60);
    fn identity(handle: HANDLE) -> BootResult<PeerIdentity> {
        unsafe {
            let pid = GetProcessId(handle);
            if pid == 0 {
                return Err(win_error(windows::core::Error::from_thread()));
            }
            let mut session = 0;
            ProcessIdToSessionId(pid, &mut session).map_err(win_error)?;
            let mut path = vec![0u16; 32768];
            let mut size = path.len() as u32;
            QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                PWSTR(path.as_mut_ptr()),
                &mut size,
            )
            .map_err(win_error)?;
            let path =
                PathBuf::from(String::from_utf16(&path[..size as usize]).map_err(crate::io_error)?);
            let image_directory = path
                .parent()
                .ok_or_else(|| crate::error(ErrorCode::UntrustedPeer, "Prozesspfad ungültig"))?
                .to_string_lossy()
                .into_owned();
            Ok(PeerIdentity {
                pid,
                session,
                sid: process_sid(handle)?,
                image_directory,
            })
        }
    }
    fn open_process(pid: u32) -> BootResult<Handle> {
        unsafe {
            Ok(Handle(
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                    false,
                    pid,
                )
                .map_err(win_error)?,
            ))
        }
    }
    fn live(handle: HANDLE) -> BootResult<()> {
        unsafe {
            if WaitForSingleObject(handle, 0) != WAIT_TIMEOUT {
                return Err(crate::error(
                    ErrorCode::UntrustedPeer,
                    "Gegenprozess ist bereits beendet",
                ));
            }
        }
        Ok(())
    }
    fn complete(handle: HANDLE, overlapped: &mut OVERLAPPED, deadline: Instant) -> BootResult<u32> {
        unsafe {
            let remaining = deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .min(u32::MAX as u128) as u32;
            let mut bytes = 0;
            if let Err(e) = GetOverlappedResultEx(handle, overlapped, &mut bytes, remaining, false)
            {
                // Cancel then drain before the OVERLAPPED/buffer can leave scope.
                let _ = CancelIoEx(handle, Some(overlapped));
                let _ = GetOverlappedResult(handle, overlapped, &mut bytes, true);
                if e.code().0 as u32 == 0x80070102 || remaining == 0 {
                    return Err(crate::error(
                        ErrorCode::Timeout,
                        "Zeitüberschreitung beim UAC-Helfer",
                    ));
                }
                return Err(win_error(e));
            }
            Ok(bytes)
        }
    }
    fn transfer(
        handle: HANDLE,
        mut buffer: &mut [u8],
        write: bool,
        deadline: Instant,
    ) -> BootResult<()> {
        unsafe {
            while !buffer.is_empty() {
                let event = Handle(CreateEventW(None, true, false, None).map_err(win_error)?);
                let mut overlap = OVERLAPPED {
                    hEvent: event.0,
                    ..Default::default()
                };
                let mut bytes = 0;
                let result = if write {
                    WriteFile(handle, Some(buffer), Some(&mut bytes), Some(&mut overlap))
                } else {
                    ReadFile(handle, Some(buffer), Some(&mut bytes), Some(&mut overlap))
                };
                match result {
                    Ok(()) => {}
                    Err(e) if e.code().0 as u32 == 0x800703e5 => {
                        bytes = complete(handle, &mut overlap, deadline)?
                    }
                    Err(e) => return Err(win_error(e)),
                }
                if bytes == 0 || bytes as usize > buffer.len() {
                    return Err(crate::error(
                        ErrorCode::InvalidMessage,
                        "Unvollständige Pipe-Nachricht",
                    ));
                }
                buffer = &mut buffer[bytes as usize..];
            }
            Ok(())
        }
    }
    fn send(handle: HANDLE, bytes: &[u8], deadline: Instant) -> BootResult<()> {
        if bytes.len() > MAX_FRAME_BYTES {
            return Err(crate::error(ErrorCode::InvalidMessage, "Nachricht zu groß"));
        }
        transfer(
            handle,
            &mut (bytes.len() as u32).to_le_bytes(),
            true,
            deadline,
        )?;
        transfer(handle, &mut bytes.to_vec(), true, deadline)
    }
    fn receive(handle: HANDLE, deadline: Instant) -> BootResult<Vec<u8>> {
        let mut length = [0; 4];
        transfer(handle, &mut length, false, deadline)?;
        let size = u32::from_le_bytes(length) as usize;
        if size == 0 || size > MAX_FRAME_BYTES {
            return Err(crate::error(
                ErrorCode::InvalidMessage,
                "Unzulässige Nachrichtengröße",
            ));
        }
        let mut bytes = vec![0; size];
        transfer(handle, &mut bytes, false, deadline)?;
        Ok(bytes)
    }
    pub(super) fn exchange(request: BrokerRequest) -> BootResult<BrokerResponse> {
        unsafe {
            let own = identity(GetCurrentProcess())?;
            let executable = std::env::current_exe().map_err(crate::io_error)?;
            let helper = executable
                .parent()
                .ok_or_else(|| crate::error(ErrorCode::NotFound, "Anwendungsverzeichnis fehlt"))?
                .join("boot-admin.exe");
            let _pins = pin_path(&helper)?;
            let nonce = Uuid::new_v4().simple().to_string();
            let pipe_name = format!(r"\\.\pipe\BootPilot-{}", Uuid::new_v4().simple());
            // Specific user rights omit FILE_CREATE_PIPE_INSTANCE (0x4).
            let descriptor = SecurityDescriptor::new(&format!(
                "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x0012019b;;;{})",
                own.sid
            ))?;
            let attributes = descriptor.attributes();
            let pipe = CreateNamedPipeW(
                PCWSTR(wide(&pipe_name).as_ptr()),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                65536,
                65536,
                60000,
                Some(&attributes),
            );
            if pipe == INVALID_HANDLE_VALUE {
                return Err(win_error(windows::core::Error::from_thread()));
            }
            let pipe = Handle(pipe);
            let file = wide(&helper);
            let args = wide(format!(
                "--pipe {pipe_name} --parent {} --nonce {nonce}",
                own.pid
            ));
            let verb = wide("runas");
            let mut info = SHELLEXECUTEINFOW {
                cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
                fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_FLAG_NO_UI,
                lpVerb: PCWSTR(verb.as_ptr()),
                lpFile: PCWSTR(file.as_ptr()),
                lpParameters: PCWSTR(args.as_ptr()),
                nShow: SW_HIDE.0,
                ..Default::default()
            };
            if let Err(e) = ShellExecuteExW(&mut info) {
                return Err(if e.code().0 as u32 == 0x800704c7 {
                    crate::error(ErrorCode::UacCancelled, "UAC-Freigabe wurde abgebrochen")
                } else {
                    win_error(e)
                });
            }
            if info.hProcess.is_invalid() {
                return Err(crate::error(
                    ErrorCode::UntrustedPeer,
                    "Helferprozess-Handle fehlt",
                ));
            }
            let process = Handle(info.hProcess);
            let helper_identity = identity(process.0)?;
            let expected = PeerIdentity {
                pid: helper_identity.pid,
                ..own.clone()
            };
            verify_peer(&expected, &helper_identity)?;
            let deadline = Instant::now() + TIMEOUT;
            let event = Handle(CreateEventW(None, true, false, None).map_err(win_error)?);
            let mut overlap = OVERLAPPED {
                hEvent: event.0,
                ..Default::default()
            };
            match ConnectNamedPipe(pipe.0, Some(&mut overlap)) {
                Ok(()) => {}
                Err(e) if e.code().0 as u32 == 0x80070217 => {}
                Err(e) if e.code().0 as u32 == 0x800703e5 => {
                    complete(pipe.0, &mut overlap, deadline)?;
                }
                Err(e) => return Err(win_error(e)),
            }
            let mut pid = 0;
            GetNamedPipeClientProcessId(pipe.0, &mut pid).map_err(win_error)?;
            if pid != helper_identity.pid {
                return Err(crate::error(
                    ErrorCode::UntrustedPeer,
                    "Falscher Pipe-Client",
                ));
            }
            live(process.0)?;
            send(
                pipe.0,
                &serde_json::to_vec(&Envelope {
                    schema_version: SCHEMA_VERSION,
                    nonce: nonce.clone(),
                    request,
                })
                .map_err(crate::io_error)?,
                deadline,
            )?;
            let reply: Reply = serde_json::from_slice(&receive(pipe.0, deadline)?)
                .map_err(|_| crate::error(ErrorCode::InvalidMessage, "Ungültige Helferantwort"))?;
            if reply.schema_version != SCHEMA_VERSION || reply.nonce != nonce {
                return Err(crate::error(
                    ErrorCode::UntrustedPeer,
                    "Antwort kann nicht zugeordnet werden",
                ));
            }
            send(pipe.0, b"ok", deadline)?;
            reply.result
        }
    }
    pub(super) fn helper() -> BootResult<()> {
        unsafe {
            let args: Vec<_> = std::env::args().skip(1).collect();
            if args.len() != 6
                || args[0] != "--pipe"
                || args[2] != "--parent"
                || args[4] != "--nonce"
                || !args[1].starts_with(r"\\.\pipe\BootPilot-")
                || args[1].len() != 51
                || args[5].len() != 32
                || !args[1][19..].chars().all(|v| v.is_ascii_hexdigit())
                || !args[5].chars().all(|v| v.is_ascii_hexdigit())
            {
                return Err(crate::error(
                    ErrorCode::InvalidRequest,
                    "Unzulässiger Helferstart",
                ));
            }
            let pid = args[3]
                .parse::<u32>()
                .map_err(|_| crate::error(ErrorCode::InvalidRequest, "Elternprozess ungültig"))?;
            let parent = open_process(pid)?;
            live(parent.0)?;
            let parent_identity = identity(parent.0)?;
            let own = identity(GetCurrentProcess())?;
            verify_peer(&PeerIdentity { pid, ..own }, &parent_identity)?;
            let mut path = vec![0u16; 32768];
            let mut size = path.len() as u32;
            QueryFullProcessImageNameW(
                parent.0,
                PROCESS_NAME_WIN32,
                PWSTR(path.as_mut_ptr()),
                &mut size,
            )
            .map_err(win_error)?;
            let path =
                PathBuf::from(String::from_utf16(&path[..size as usize]).map_err(crate::io_error)?);
            if ![
                "bootpilot.exe",
                "boot-cli.exe",
                "boot-mcp.exe",
                "bootpilot-read-probe.exe",
            ]
            .iter()
            .any(|s| {
                path.file_name()
                    .is_some_and(|v| v.to_string_lossy().eq_ignore_ascii_case(s))
            }) {
                return Err(crate::error(
                    ErrorCode::UntrustedPeer,
                    "Aufrufendes Programm ist nicht freigegeben",
                ));
            }
            let _pins = pin_path(&path)?;
            let pipe = Handle(
                CreateFileW(
                    PCWSTR(wide(&args[1]).as_ptr()),
                    GENERIC_READ.0 | GENERIC_WRITE.0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    None,
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED,
                    None,
                )
                .map_err(win_error)?,
            );
            let mut server = 0;
            GetNamedPipeServerProcessId(pipe.0, &mut server).map_err(win_error)?;
            if server != pid {
                return Err(crate::error(
                    ErrorCode::UntrustedPeer,
                    "Falscher Pipe-Server",
                ));
            }
            live(parent.0)?;
            let deadline = Instant::now() + TIMEOUT;
            let request = decode_request(&receive(pipe.0, deadline)?, &args[5])?;
            let result = dispatch(request, &parent_identity.sid);
            send(
                pipe.0,
                &serde_json::to_vec(&Reply {
                    schema_version: SCHEMA_VERSION,
                    nonce: args[5].clone(),
                    result,
                })
                .map_err(crate::io_error)?,
                Instant::now() + TIMEOUT,
            )?;
            // Keep the short-lived client connected until the server has consumed the response.
            if receive(pipe.0, Instant::now() + TIMEOUT)? != b"ok" {
                return Err(crate::error(ErrorCode::InvalidMessage, "Bestätigung fehlt"));
            }
            Ok(())
        }
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        fn server() -> (String, Handle) {
            unsafe {
                let name = format!(r"\\.\pipe\BootPilot-test-{}", Uuid::new_v4().simple());
                let sid = process_sid(GetCurrentProcess()).unwrap();
                let sd = SecurityDescriptor::new(&format!(
                    "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x0012019b;;;{sid})"
                ))
                .unwrap();
                let attrs = sd.attributes();
                let handle = CreateNamedPipeW(
                    PCWSTR(wide(&name).as_ptr()),
                    PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                    1,
                    4096,
                    4096,
                    1000,
                    Some(&attrs),
                );
                assert_ne!(handle, INVALID_HANDLE_VALUE);
                (name, Handle(handle))
            }
        }
        fn client(name: &str) -> Handle {
            unsafe {
                Handle(
                    CreateFileW(
                        PCWSTR(wide(name).as_ptr()),
                        FILE_GENERIC_READ.0 | FILE_WRITE_DATA.0,
                        FILE_SHARE_READ | FILE_SHARE_WRITE,
                        None,
                        OPEN_EXISTING,
                        FILE_FLAG_OVERLAPPED,
                        None,
                    )
                    .unwrap(),
                )
            }
        }
        fn connected(handle: HANDLE) {
            unsafe {
                let event = Handle(CreateEventW(None, true, false, None).unwrap());
                let mut overlap = OVERLAPPED {
                    hEvent: event.0,
                    ..Default::default()
                };
                match ConnectNamedPipe(handle, Some(&mut overlap)) {
                    Ok(()) => {}
                    Err(e) if e.code().0 as u32 == 0x80070217 => {}
                    Err(e) if e.code().0 as u32 == 0x800703e5 => {
                        complete(
                            handle,
                            &mut overlap,
                            Instant::now() + Duration::from_secs(3),
                        )
                        .unwrap();
                    }
                    Err(e) => panic!("{e}"),
                }
            }
        }
        #[test]
        fn oversized_wire_length_is_rejected_without_reading_body() {
            let (name, pipe) = server();
            let (release, wait) = std::sync::mpsc::channel();
            let thread = std::thread::spawn(move || {
                let pipe = client(&name);
                transfer(
                    pipe.0,
                    &mut ((MAX_FRAME_BYTES + 1) as u32).to_le_bytes(),
                    true,
                    Instant::now() + Duration::from_secs(3),
                )
                .unwrap();
                wait.recv().unwrap();
            });
            connected(pipe.0);
            let error = receive(pipe.0, Instant::now() + Duration::from_secs(3)).unwrap_err();
            release.send(()).unwrap();
            thread.join().unwrap();
            assert_eq!(error.code, ErrorCode::InvalidMessage);
        }
        #[test]
        fn stalled_wire_read_times_out_and_drains_cancellation() {
            let (name, pipe) = server();
            let (release, wait) = std::sync::mpsc::channel();
            let thread = std::thread::spawn(move || {
                let _pipe = client(&name);
                wait.recv().unwrap();
            });
            connected(pipe.0);
            let error = receive(pipe.0, Instant::now() + Duration::from_millis(20)).unwrap_err();
            release.send(()).unwrap();
            thread.join().unwrap();
            assert_eq!(error.code, ErrorCode::Timeout);
        }
    }
}
