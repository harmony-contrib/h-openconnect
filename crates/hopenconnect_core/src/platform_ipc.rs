use crate::platform_state::{BrowserOpenRequest, PlatformVpnState, SessionHandoff};
use ohos_ashmem_binding::Ashmem;
use serde::{Deserialize, Serialize};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

// Keep the memory layout aligned with Paws PR #7. Each process owns a lane and
// alternates between two slots, so readers never consume a frame being
// replaced by the sibling process.
const REGION_SIZE: usize = 8 * 1024 * 1024;
const REGION_HEADER_SIZE: usize = 4096;
const UI_LANE_SIZE: usize = 1024 * 1024;
const FRAME_HEADER_SIZE: usize = 32;
const FRAME_MAGIC: u32 = 0x4841_4E59;
const REGION_MAGIC: &[u8; 8] = b"HANYIPC\0";
// Version 3 changes the notification descriptor from a transferred socketpair
// endpoint to a UI-owned listener. The VPN process creates its own stream in
// its HarmonyOS security domain and connects using the session id in ashmem.
const PROTOCOL_VERSION: u32 = 3;
const SLOT_COUNT: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlatformRole {
    Ui,
    Vpn,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct PlatformEnvelope {
    pub(crate) state: Option<PlatformVpnState>,
    pub(crate) session_handoff: Option<SessionHandoff>,
    pub(crate) browser_request: Option<BrowserOpenRequest>,
    pub(crate) browser_request_ack: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum PlatformIpcError {
    #[error("platform shared memory lock is poisoned")]
    LockPoisoned,
    #[error("invalid platform shared memory header")]
    InvalidHeader,
    #[error("platform shared memory frame is too large: {actual} bytes, maximum {maximum}")]
    FrameTooLarge { actual: usize, maximum: usize },
    #[error("serialize platform shared memory frame failed: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("platform shared memory operation failed: {0}")]
    Memory(String),
    #[error("platform change notification failed: {0}")]
    Notification(String),
}

type Result<T> = std::result::Result<T, PlatformIpcError>;

pub(crate) struct PlatformSharedMemoryFds {
    pub(crate) ashmem: RawFd,
    pub(crate) notification: RawFd,
}

pub(crate) struct PlatformIpc {
    memory: Mutex<Ashmem>,
    role: PlatformRole,
    published: Mutex<PlatformEnvelope>,
    next_generation: AtomicU64,
    notification: SocketNotification,
}

struct SocketNotification {
    listener: Option<OwnedFd>,
    address: NotificationAddress,
    connection: Mutex<Option<Arc<OwnedFd>>>,
    subscription: Mutex<()>,
    cancel_read: OwnedFd,
    cancel_write: OwnedFd,
    cancel_pending: AtomicBool,
}

impl PlatformIpc {
    pub(crate) fn create_ui() -> Result<(Arc<Self>, PlatformSharedMemoryFds)> {
        let mut ashmem = Ashmem::create("hopenconnect-platform-session", REGION_SIZE)
            .map_err(|error| PlatformIpcError::Memory(error.to_string()))?;
        ashmem
            .map_read_write()
            .map_err(|error| PlatformIpcError::Memory(error.to_string()))?;
        let notification = SocketNotification::listen(session_id())?;
        let fds = PlatformSharedMemoryFds {
            ashmem: ashmem.as_raw_fd(),
            notification: notification.listener_fd()?,
        };
        let ipc = Arc::new(Self {
            memory: Mutex::new(ashmem),
            role: PlatformRole::Ui,
            published: Mutex::new(PlatformEnvelope::default()),
            next_generation: AtomicU64::new(1),
            notification,
        });
        ipc.initialize_region()?;
        Ok((ipc, fds))
    }

    pub(crate) fn attach_vpn_raw(ashmem_fd: RawFd, notification_fd: RawFd) -> Result<Arc<Self>> {
        if ashmem_fd < 0 || notification_fd < 0 {
            return Err(PlatformIpcError::InvalidHeader);
        }
        // ArkTS owns the Want descriptors. Duplicate ashmem for this binding.
        // The notification descriptor only identifies the UI listener; the
        // VPN process must create its own endpoint in its security domain.
        let ashmem_fd = duplicate_fd(ashmem_fd)?;
        let mut ashmem = Ashmem::from_owned_fd(ashmem_fd)
            .map_err(|error| PlatformIpcError::Memory(error.to_string()))?;
        if ashmem.size() != REGION_SIZE {
            return Err(PlatformIpcError::InvalidHeader);
        }
        ashmem
            .map_read_write()
            .map_err(|error| PlatformIpcError::Memory(error.to_string()))?;
        let mut ipc = Self {
            memory: Mutex::new(ashmem),
            role: PlatformRole::Vpn,
            published: Mutex::new(PlatformEnvelope::default()),
            next_generation: AtomicU64::new(1),
            notification: SocketNotification::connect_lazily(0)?,
        };
        ipc.validate_region()?;
        let header = ipc.read_memory(0, 32)?;
        ipc.notification.address = NotificationAddress::new(read_u64(&header[16..24]));
        ipc.seed_next_generation()?;
        Ok(Arc::new(ipc))
    }

    pub(crate) fn ui_fds(&self) -> Result<PlatformSharedMemoryFds> {
        if self.role != PlatformRole::Ui {
            return Err(PlatformIpcError::InvalidHeader);
        }
        let ashmem = self
            .memory
            .lock()
            .map_err(|_| PlatformIpcError::LockPoisoned)?
            .as_raw_fd();
        let notification = self.notification.listener_fd()?;
        Ok(PlatformSharedMemoryFds {
            ashmem,
            notification,
        })
    }

    pub(crate) fn publish_snapshot(
        &self,
        state: PlatformVpnState,
        session_handoff: Option<SessionHandoff>,
        browser_request: Option<BrowserOpenRequest>,
        browser_request_ack: Option<String>,
    ) -> Result<()> {
        let (envelope, scrub_previous_payload) = {
            let mut published = self
                .published
                .lock()
                .map_err(|_| PlatformIpcError::LockPoisoned)?;
            let scrub_previous_payload = replace_snapshot(
                &mut published,
                state,
                session_handoff,
                browser_request,
                browser_request_ack,
            );
            (published.clone(), scrub_previous_payload)
        };
        self.publish(&envelope)?;
        if scrub_previous_payload {
            // Frames alternate between two slots. Publish the cleared payload
            // twice so neither slot retains an authenticated cookie or SSO URL.
            self.publish(&envelope)?;
        }
        Ok(())
    }

    pub(crate) fn publish_state(&self, state: PlatformVpnState) -> Result<()> {
        let envelope = {
            let mut published = self
                .published
                .lock()
                .map_err(|_| PlatformIpcError::LockPoisoned)?;
            published.state = Some(state);
            published.clone()
        };
        self.publish(&envelope)
    }

    pub(crate) fn read_remote(&self) -> Result<Option<PlatformEnvelope>> {
        let remote_role = match self.role {
            PlatformRole::Ui => PlatformRole::Vpn,
            PlatformRole::Vpn => PlatformRole::Ui,
        };
        self.read_lane(remote_role)
    }

    pub(crate) fn wait_for_change(&self, timeout: Duration) -> Result<bool> {
        self.notification.wait(Some(timeout))
    }

    /// Block until the peer publishes a new frame.
    ///
    /// Fully event driven: the wait parks on the session notification socket
    /// together with a process-local cancellation socket. It resolves when a
    /// frame arrives (`Ok(true)`) or when [`cancel_event_waits`] is invoked
    /// (`Ok(false)`); it never polls.
    pub(crate) fn wait_for_change_event_cancellable(&self) -> Result<bool> {
        self.notification.wait_event_cancellable()
    }

    pub(crate) fn is_ui(&self) -> bool {
        self.role == PlatformRole::Ui
    }

    pub(crate) fn cancel_event_waits(&self) {
        self.notification.cancel_waits();
    }

    fn publish(&self, envelope: &PlatformEnvelope) -> Result<()> {
        let content = serde_json::to_vec(envelope)?;
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        self.write_frame(self.role, generation, &content)?;
        self.notification.notify()
    }

    fn initialize_region(&self) -> Result<()> {
        let mut header = [0_u8; 32];
        header[..REGION_MAGIC.len()].copy_from_slice(REGION_MAGIC);
        header[8..12].copy_from_slice(&PROTOCOL_VERSION.to_le_bytes());
        header[12..16].copy_from_slice(&(REGION_SIZE as u32).to_le_bytes());
        header[16..24].copy_from_slice(&self.notification.address.session.to_le_bytes());
        let header_checksum = checksum(&header[..24]);
        header[24..28].copy_from_slice(&header_checksum.to_le_bytes());
        self.write_memory(0, &header)
    }

    fn validate_region(&self) -> Result<()> {
        let header = self.read_memory(0, 32)?;
        if header[..REGION_MAGIC.len()] != REGION_MAGIC[..]
            || read_u32(&header[8..12]) != PROTOCOL_VERSION
            || read_u32(&header[12..16]) as usize != REGION_SIZE
            || read_u32(&header[24..28]) != checksum(&header[..24])
        {
            return Err(PlatformIpcError::InvalidHeader);
        }
        Ok(())
    }

    fn seed_next_generation(&self) -> Result<()> {
        let latest = self.latest_generation(self.role)?.unwrap_or(0);
        self.next_generation
            .store(latest.saturating_add(1), Ordering::Relaxed);
        Ok(())
    }

    fn write_frame(&self, role: PlatformRole, generation: u64, content: &[u8]) -> Result<()> {
        let (lane_offset, slot_size) = lane_layout(role);
        let maximum = slot_size - FRAME_HEADER_SIZE;
        if content.len() > maximum {
            return Err(PlatformIpcError::FrameTooLarge {
                actual: content.len(),
                maximum,
            });
        }
        let slot_offset = lane_offset + generation as usize % SLOT_COUNT * slot_size;
        let previous_length = self
            .read_memory(slot_offset, FRAME_HEADER_SIZE)
            .ok()
            .and_then(|header| FrameHeader::parse(&header, slot_size))
            .map(|header| header.content_length)
            .unwrap_or_default();
        self.write_memory(slot_offset + FRAME_HEADER_SIZE, content)?;
        if previous_length > content.len() {
            self.write_memory(
                slot_offset + FRAME_HEADER_SIZE + content.len(),
                &vec![0; previous_length - content.len()],
            )?;
        }

        let mut header = [0_u8; FRAME_HEADER_SIZE];
        header[0..4].copy_from_slice(&FRAME_MAGIC.to_le_bytes());
        header[4..8].copy_from_slice(&PROTOCOL_VERSION.to_le_bytes());
        header[8..16].copy_from_slice(&generation.to_le_bytes());
        header[16..20].copy_from_slice(&(content.len() as u32).to_le_bytes());
        header[20..24].copy_from_slice(&checksum(content).to_le_bytes());
        let header_checksum = checksum(&header[..24]);
        header[24..28].copy_from_slice(&header_checksum.to_le_bytes());
        self.write_memory(slot_offset, &header)
    }

    fn read_lane(&self, role: PlatformRole) -> Result<Option<PlatformEnvelope>> {
        let (lane_offset, slot_size) = lane_layout(role);
        let mut latest: Option<(u64, Vec<u8>)> = None;
        for slot_index in 0..SLOT_COUNT {
            let slot_offset = lane_offset + slot_index * slot_size;
            let Some((generation, content)) = self.read_frame(slot_offset, slot_size)? else {
                continue;
            };
            if latest
                .as_ref()
                .is_none_or(|(current, _)| generation > *current)
            {
                latest = Some((generation, content));
            }
        }
        latest
            .map(|(_, content)| serde_json::from_slice(&content).map_err(PlatformIpcError::from))
            .transpose()
    }

    fn latest_generation(&self, role: PlatformRole) -> Result<Option<u64>> {
        let (lane_offset, slot_size) = lane_layout(role);
        let mut latest = None;
        for slot_index in 0..SLOT_COUNT {
            let slot_offset = lane_offset + slot_index * slot_size;
            let header = self.read_memory(slot_offset, FRAME_HEADER_SIZE)?;
            if let Some(frame) = FrameHeader::parse(&header, slot_size) {
                latest =
                    Some(latest.map_or(frame.generation, |value: u64| value.max(frame.generation)));
            }
        }
        Ok(latest)
    }

    fn read_frame(&self, slot_offset: usize, slot_size: usize) -> Result<Option<(u64, Vec<u8>)>> {
        for _ in 0..3 {
            let first = self.read_memory(slot_offset, FRAME_HEADER_SIZE)?;
            let Some(header) = FrameHeader::parse(&first, slot_size) else {
                // Payload is committed before its header. Retry a torn header
                // instead of accepting a partially published frame.
                continue;
            };
            let content =
                self.read_memory(slot_offset + FRAME_HEADER_SIZE, header.content_length)?;
            let second = self.read_memory(slot_offset, FRAME_HEADER_SIZE)?;
            if first == second && checksum(&content) == header.content_checksum {
                return Ok(Some((header.generation, content)));
            }
        }
        Ok(None)
    }

    fn read_memory(&self, offset: usize, length: usize) -> Result<Vec<u8>> {
        self.memory
            .lock()
            .map_err(|_| PlatformIpcError::LockPoisoned)?
            .read(offset, length)
            .map_err(|error| PlatformIpcError::Memory(error.to_string()))
    }

    fn write_memory(&self, offset: usize, data: &[u8]) -> Result<()> {
        self.memory
            .lock()
            .map_err(|_| PlatformIpcError::LockPoisoned)?
            .write(offset, data)
            .map_err(|error| PlatformIpcError::Memory(error.to_string()))
    }
}

fn replace_snapshot(
    published: &mut PlatformEnvelope,
    state: PlatformVpnState,
    session_handoff: Option<SessionHandoff>,
    browser_request: Option<BrowserOpenRequest>,
    browser_request_ack: Option<String>,
) -> bool {
    let scrub_previous_payload = (published.session_handoff.is_some() && session_handoff.is_none())
        || (published.browser_request.is_some() && browser_request.is_none());
    published.state = Some(state);
    published.session_handoff = session_handoff;
    published.browser_request = browser_request;
    published.browser_request_ack = browser_request_ack;
    scrub_previous_payload
}

struct NotificationAddress {
    session: u64,
    raw: libc::sockaddr_un,
    length: libc::socklen_t,
}

impl NotificationAddress {
    fn new(session: u64) -> Self {
        // SAFETY: a zeroed sockaddr_un is a valid base value before setting
        // its family and path fields below.
        let mut raw: libc::sockaddr_un = unsafe { std::mem::zeroed() };
        raw.sun_family = libc::AF_UNIX as libc::sa_family_t;
        let name = format!("\0hopenconnect-platform-{session:016x}");
        for (target, byte) in raw.sun_path.iter_mut().zip(name.bytes()) {
            *target = byte as libc::c_char;
        }
        let length = std::mem::offset_of!(libc::sockaddr_un, sun_path) + name.len();
        Self {
            session,
            raw,
            length: length as libc::socklen_t,
        }
    }

    fn pointer(&self) -> *const libc::sockaddr {
        (&self.raw as *const libc::sockaddr_un).cast()
    }
}

impl SocketNotification {
    fn listen(session: u64) -> Result<Self> {
        let address = NotificationAddress::new(session);
        let listener = Self::create_socket()?;
        // SAFETY: listener is a live Unix stream socket and address points to
        // an initialized sockaddr_un with its exact platform length.
        if unsafe { libc::bind(listener.as_raw_fd(), address.pointer(), address.length) } < 0
            // SAFETY: the bound descriptor remains live for listen.
            || unsafe { libc::listen(listener.as_raw_fd(), 16) } < 0
        {
            return Err(notification_error());
        }
        configure_nonblocking(listener.as_raw_fd())?;
        Self::new(Some(listener), address, None)
    }

    fn connect_lazily(session: u64) -> Result<Self> {
        // Want validation only maps ashmem. Connect when this binding actually
        // publishes or subscribes so a probe cannot displace the live owner.
        Self::new(None, NotificationAddress::new(session), None)
    }

    fn new(
        listener: Option<OwnedFd>,
        address: NotificationAddress,
        connection: Option<OwnedFd>,
    ) -> Result<Self> {
        let (cancel_read, cancel_write) = create_notification_pair()?;
        Ok(Self {
            listener,
            address,
            connection: Mutex::new(connection.map(Arc::new)),
            subscription: Mutex::new(()),
            cancel_read,
            cancel_write,
            cancel_pending: AtomicBool::new(false),
        })
    }

    fn listener_fd(&self) -> Result<RawFd> {
        self.listener
            .as_ref()
            .map(AsRawFd::as_raw_fd)
            .ok_or(PlatformIpcError::InvalidHeader)
    }

    fn create_socket() -> Result<OwnedFd> {
        // SAFETY: socket has no borrowed pointers and returns a new descriptor.
        let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
        if fd < 0 {
            return Err(notification_error());
        }
        // SAFETY: a non-negative socket result transfers sole descriptor
        // ownership to this value.
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }

    fn connection(&self) -> Result<Option<Arc<OwnedFd>>> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| PlatformIpcError::LockPoisoned)?;
        if let Some(listener) = &self.listener {
            loop {
                // SAFETY: listener is live and no peer address is requested.
                let fd = unsafe {
                    libc::accept(
                        listener.as_raw_fd(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                    )
                };
                if fd >= 0 {
                    // SAFETY: accept returned a fresh owned descriptor.
                    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
                    configure_nonblocking(fd.as_raw_fd())?;
                    *connection = Some(Arc::new(fd));
                    // A publisher can accept while the waiter polls only the
                    // listener. Wake it so it rebuilds the descriptor set.
                    self.wake_local_waiter();
                    continue;
                }
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                if error.kind() == io::ErrorKind::WouldBlock {
                    break;
                }
                return Err(PlatformIpcError::Notification(error.to_string()));
            }
        } else if connection.is_none() {
            // Each process creates its endpoint in its own SELinux domain.
            // A UI-created socketpair passed through Want is not writable by
            // vpn_isolate_hap on affected HarmonyOS releases.
            let fd = Self::create_socket()?;
            // SAFETY: fd is a live Unix stream socket and address is valid for
            // the full duration of connect.
            if unsafe { libc::connect(fd.as_raw_fd(), self.address.pointer(), self.address.length) }
                < 0
            {
                return Err(notification_error());
            }
            configure_nonblocking(fd.as_raw_fd())?;
            *connection = Some(Arc::new(fd));
        }
        Ok(connection.clone())
    }

    fn notify(&self) -> Result<()> {
        let Some(connection) = self.connection()? else {
            // UI may publish before the Extension attaches. The latest frame
            // remains in ashmem and is read when the peer connects.
            return Ok(());
        };
        let value = [1_u8];
        loop {
            // SAFETY: connection is a live descriptor and value remains valid
            // for the duration of send.
            let written = unsafe {
                libc::send(
                    connection.as_raw_fd(),
                    value.as_ptr().cast(),
                    value.len(),
                    libc::MSG_NOSIGNAL,
                )
            };
            if written == value.len() as isize {
                return Ok(());
            }
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            if error.kind() == io::ErrorKind::WouldBlock {
                return Ok(());
            }
            if self.listener.is_some()
                && matches!(error.raw_os_error(), Some(libc::EPIPE | libc::ECONNRESET))
            {
                self.clear_connection(&connection)?;
                return Ok(());
            }
            return Err(PlatformIpcError::Notification(error.to_string()));
        }
    }

    fn clear_connection(&self, expected: &Arc<OwnedFd>) -> Result<()> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| PlatformIpcError::LockPoisoned)?;
        if connection
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, expected))
        {
            *connection = None;
        }
        Ok(())
    }

    fn wait(&self, timeout: Option<Duration>) -> Result<bool> {
        self.wait_internal(timeout, false)
    }

    fn wait_event_cancellable(&self) -> Result<bool> {
        self.wait_internal(None, true)
    }

    fn wait_internal(&self, timeout: Option<Duration>, cancellable: bool) -> Result<bool> {
        let _subscription = self
            .subscription
            .lock()
            .map_err(|_| PlatformIpcError::LockPoisoned)?;
        let cancel_fd = self.cancel_read.as_raw_fd();
        if cancellable && self.cancel_pending.swap(false, Ordering::AcqRel) {
            drain_cancel_fd(cancel_fd);
            return Ok(false);
        }
        let timeout_ms = timeout
            .map(|timeout| timeout.as_millis().min(i32::MAX as u128) as i32)
            .unwrap_or(-1);
        loop {
            // Keep the polled connection alive even if another thread accepts
            // a replacement endpoint and updates the active Arc.
            let connection = self.connection()?;
            let mut descriptors = [
                libc::pollfd {
                    fd: connection.as_ref().map_or(-1, |fd| fd.as_raw_fd()),
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: self.listener.as_ref().map_or(-1, AsRawFd::as_raw_fd),
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: cancel_fd,
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            // SAFETY: all three pollfd entries are initialized; negative fds
            // are explicitly ignored by poll.
            let ready = unsafe { libc::poll(descriptors.as_mut_ptr(), 3, timeout_ms) };
            if ready == 0 {
                return Ok(false);
            }
            if ready < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(PlatformIpcError::Notification(error.to_string()));
            }
            if descriptors[2].revents != 0 {
                drain_cancel_fd(cancel_fd);
                if cancellable && self.cancel_pending.swap(false, Ordering::AcqRel) {
                    return Ok(false);
                }
                continue;
            }
            if descriptors[1].revents != 0 {
                // Attachment itself wakes readers so they consume the latest
                // ashmem lane even if the peer has not published a new frame.
                self.connection()?;
                return Ok(true);
            }
            if descriptors[0].revents != 0 {
                if let Some(connection) = connection {
                    let (changed, closed) = drain_notifications(connection.as_raw_fd())?;
                    if closed {
                        self.clear_connection(&connection)?;
                        if self.listener.is_none() {
                            return Err(PlatformIpcError::Notification(
                                "platform notification peer closed".to_owned(),
                            ));
                        }
                        // Wake UI once for peer loss, then wait on the listener
                        // for a future exact owner rather than spinning.
                        return Ok(true);
                    }
                    if changed {
                        return Ok(true);
                    }
                }
            }
        }
    }

    fn cancel_waits(&self) {
        if self.cancel_pending.swap(true, Ordering::AcqRel) {
            return;
        }
        self.wake_local_waiter();
    }

    fn wake_local_waiter(&self) {
        let value = [1_u8];
        // SAFETY: cancel_write is live and value remains valid for send.
        unsafe {
            libc::send(
                self.cancel_write.as_raw_fd(),
                value.as_ptr().cast(),
                value.len(),
                libc::MSG_NOSIGNAL,
            );
        }
    }
}

fn notification_error() -> PlatformIpcError {
    PlatformIpcError::Notification(io::Error::last_os_error().to_string())
}

fn drain_cancel_fd(fd: RawFd) {
    let mut buffer = [0_u8; 64];
    loop {
        // SAFETY: `fd` is a live nonblocking datagram descriptor and `buffer`
        // is valid writable storage for the supplied length.
        let read = unsafe { libc::recv(fd, buffer.as_mut_ptr().cast(), buffer.len(), 0) };
        if read > 0 {
            continue;
        }
        if read == 0 {
            return;
        }
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::Interrupted {
            continue;
        }
        return;
    }
}

struct FrameHeader {
    generation: u64,
    content_length: usize,
    content_checksum: u32,
}

impl FrameHeader {
    fn parse(bytes: &[u8], slot_size: usize) -> Option<Self> {
        if bytes.len() != FRAME_HEADER_SIZE
            || read_u32(&bytes[0..4]) != FRAME_MAGIC
            || read_u32(&bytes[4..8]) != PROTOCOL_VERSION
            || read_u32(&bytes[24..28]) != checksum(&bytes[..24])
        {
            return None;
        }
        let content_length = read_u32(&bytes[16..20]) as usize;
        if content_length > slot_size - FRAME_HEADER_SIZE {
            return None;
        }
        Some(Self {
            generation: read_u64(&bytes[8..16]),
            content_length,
            content_checksum: read_u32(&bytes[20..24]),
        })
    }
}

const fn lane_layout(role: PlatformRole) -> (usize, usize) {
    match role {
        PlatformRole::Ui => (REGION_HEADER_SIZE, UI_LANE_SIZE / SLOT_COUNT),
        PlatformRole::Vpn => {
            let offset = REGION_HEADER_SIZE + UI_LANE_SIZE;
            (offset, (REGION_SIZE - offset) / SLOT_COUNT)
        }
    }
}

fn create_notification_pair() -> Result<(OwnedFd, OwnedFd)> {
    let mut fds = [-1; 2];
    // SAFETY: `fds` provides writable storage for exactly two descriptors as
    // required by `socketpair`.
    let result = unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr()) };
    if result < 0 {
        return Err(PlatformIpcError::Notification(
            io::Error::last_os_error().to_string(),
        ));
    }
    // SAFETY: a successful `socketpair` call initialized both descriptors and
    // transfers their sole ownership to these `OwnedFd` values.
    let first = unsafe { OwnedFd::from_raw_fd(fds[0]) };
    // SAFETY: same ownership guarantee as `first`, for the second descriptor.
    let second = unsafe { OwnedFd::from_raw_fd(fds[1]) };
    configure_nonblocking(first.as_raw_fd())?;
    configure_nonblocking(second.as_raw_fd())?;
    Ok((first, second))
}

fn configure_nonblocking(fd: RawFd) -> Result<()> {
    // SAFETY: callers provide a live descriptor owned by this module.
    let status = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if status < 0 {
        return Err(PlatformIpcError::Notification(
            io::Error::last_os_error().to_string(),
        ));
    }
    // SAFETY: the same live descriptor and flags returned by `F_GETFL` are
    // passed back to `fcntl`, with only `O_NONBLOCK` added.
    let set_status = unsafe { libc::fcntl(fd, libc::F_SETFL, status | libc::O_NONBLOCK) };
    if set_status < 0 {
        return Err(PlatformIpcError::Notification(
            io::Error::last_os_error().to_string(),
        ));
    }
    // SAFETY: callers provide a live descriptor owned by this module.
    let descriptor = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if descriptor < 0 {
        return Err(PlatformIpcError::Notification(
            io::Error::last_os_error().to_string(),
        ));
    }
    // SAFETY: the same live descriptor and flags returned by `F_GETFD` are
    // passed back to `fcntl`, with only `FD_CLOEXEC` added.
    let set_descriptor = unsafe { libc::fcntl(fd, libc::F_SETFD, descriptor | libc::FD_CLOEXEC) };
    if set_descriptor < 0 {
        return Err(PlatformIpcError::Notification(
            io::Error::last_os_error().to_string(),
        ));
    }
    Ok(())
}

fn duplicate_fd(fd: RawFd) -> Result<OwnedFd> {
    // SAFETY: callers provide a live descriptor; `dup` creates an independent
    // descriptor whose ownership is transferred below on success.
    let duplicate = unsafe { libc::dup(fd) };
    if duplicate < 0 {
        return Err(PlatformIpcError::Notification(
            io::Error::last_os_error().to_string(),
        ));
    }
    // SAFETY: a non-negative result from `dup` is a new owned descriptor.
    Ok(unsafe { OwnedFd::from_raw_fd(duplicate) })
}

fn drain_notifications(fd: RawFd) -> Result<(bool, bool)> {
    let mut changed = false;
    let mut buffer = [0_u8; 64];
    loop {
        // SAFETY: `fd` is a live nonblocking stream descriptor and `buffer`
        // is valid writable storage for the supplied length.
        let read = unsafe { libc::recv(fd, buffer.as_mut_ptr().cast(), buffer.len(), 0) };
        if read > 0 {
            changed = true;
            continue;
        }
        if read == 0 {
            return Ok((changed, true));
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ECONNRESET) {
            return Ok((changed, true));
        }
        if error.kind() == io::ErrorKind::Interrupted {
            continue;
        }
        if error.kind() == io::ErrorKind::WouldBlock {
            return Ok((changed, false));
        }
        return Err(PlatformIpcError::Notification(error.to_string()));
    }
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c_9dc5, |hash, byte| {
        hash.wrapping_mul(0x0100_0193) ^ u32::from(*byte)
    })
}

fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes.try_into().expect("validated u32 slice"))
}

fn read_u64(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes.try_into().expect("validated u64 slice"))
}

fn session_id() -> u64 {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or_default();
    timestamp ^ u64::from(std::process::id()).rotate_left(32)
}

#[cfg(test)]
mod tests;
