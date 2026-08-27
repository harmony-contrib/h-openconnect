# H-OpenConnect English (en-US) catalog — Fluent

# --- Navigation / status (was UiStrings) ---
nav-home = Home
nav-connections = Connections
nav-statistics = Statistics
nav-more = More
home-title = Secure Connection
connect = Connect
disconnect = Disconnect
connecting = Connecting…
disconnecting = Disconnecting…
connected = Connected
disconnected = Not connected
failed = Connection failed
select-connection = Select a connection
no-connection = No connection selected
current = Active
server = Server address
group = Group
protocol = Primary protocol
username = Username
password = Password
name = Connection name
auth-method = Authentication
certificate = Certificate
backup-servers = Backup servers
basic = Basics
strict-cert = Strict certificate trust
block-untrusted = Block untrusted servers
local-lan = Allow local LAN access
force-global = Force global routing
force-global-desc = Ignore split-tunnel; send all IPv4 through the tunnel
connect-on-demand = Reconnect after unexpected disconnect
external-browser = External browser auth (SAML)
fips-mode = FIPS mode
mtu-override = MTU override
cancel = Cancel
add-connection = Add connection
edit-connection = Edit connection
favorite = Favorite
appearance = Appearance
diagnostics = Logs
logs-search-placeholder = Keyword, level, or time
logs-empty-title = No logs
logs-empty-subtitle = Core and session events will appear here.
logs-level-all = All
about = About
language = Language
theme = Theme
system = System
light = Light
dark = Dark
assigned-ip = Assigned IP
duration = Duration
sent = Sent
received = Received
gateway = Gateway
mtu = MTU
packets-sent = Packets sent
packets-received = Packets received
version = Version
sdk-status = Stack
sdk-pending = Not linked
sdk-ready = OpenConnect linked
empty-connections = No connections yet. Tap + to add one.
form-required = Name and server are required
feedback-connected = Connected
feedback-disconnected = Disconnected
feedback-failed = Connection failed. Check the error and diagnostics.
feedback-deleted = Connection deleted
auth-password = Password
auth-certificate = Certificate
auth-password-cert = Password+Cert
auth-saml = SAML
mtu-auto = Auto
challenge-submit = Continue

# --- Lifecycle ---
lifecycle-authenticating = Authenticating…
lifecycle-establishing = Establishing tunnel…

# --- Toasts (state.rs / tasks.rs) ---
toast-open-browser-failed = Could not open browser
toast-no-auth-form = No authentication form is waiting
toast-enter-otp = Enter the OTP / SMS code, then tap Continue
toast-group-fallback = The configured group is unavailable; using the server default
toast-group-fetch-failed = Could not fetch groups; manual entry is available
toast-open-link-failed-prefix = Could not open link:
toast-log-started = Log recording started
toast-log-stopped = Log recording stopped
toast-log-toggle-failed-prefix = Failed to change log recording:
toast-log-exported-prefix = Log exported:
toast-log-export-failed-prefix = Failed to export log:
toast-log-deleted-prefix = Log deleted:
toast-log-delete-failed-prefix = Failed to delete log:
toast-enter-password = Enter the password first
toast-select-cert = Select a client certificate file first

# --- About ---
about-not-linked = Not linked
about-tagline = Secure remote access client for HarmonyOS
about-application = Application
about-open-source = Open source & licenses
about-privacy = Privacy
about-privacy-storage = Connection profiles and credentials stay in the app-private directory and are excluded from system backup.
about-privacy-logs = Diagnostic recording is off by default and writes local daily archives only after you enable it.
about-privacy-no-telemetry = The app contains no analytics or telemetry upload; network requests are initiated by your configured gateway and authentication flow.
about-privacy-device-id = After privacy consent, the app reads ODID and provides it as a terminal identifier only to the VPN gateway you actively connect to. The app does not collect OAID.
about-disclaimer = H-OpenConnect is an independent open-source project and is not affiliated with or endorsed by Cisco; related names and trademarks belong to their respective owners.

# --- Home ---
home-live-session = Live AnyConnect / OpenConnect session
home-not-linked = OpenConnect is not linked in this build

# --- Challenge ---
challenge-required = Server requires additional authentication
challenge-round = Authentication form · round { $n }
challenge-placeholder = Type here

# --- Statistics ---
statistics-hint = Live traffic and assigned address appear after connect

# --- Settings ---
settings-preferences = Preferences
settings-language-theme = Language and light / dark theme
settings-operations = Operations
settings-logs = Inspect OpenConnect runtime logs
settings-about = Open source, component versions and privacy
settings-language-hint = Choose the interface language; System follows device changes
settings-theme-hint = Use light, dark, or the system appearance; changes apply immediately

# --- Connections editor ---
conn-fetching-groups = Fetching server groups…
conn-reading-groups = Reading AnyConnect authentication groups
conn-browse = Browse
conn-saml-login = SAML login in system browser
conn-pin-top = Pin near the top of the list
conn-advanced = Advanced
conn-advanced-desc = When off, uncommon options stay hidden and use defaults
conn-show-advanced = Show advanced options
conn-advanced-detail = Protocol, cert details, proxy, split tunnel, tokens…
conn-cert-path-placeholder = Client certificate path PEM/P12
conn-private-key-path = Private key path (optional)
conn-key-password = Key password (PKCS#12/PEM)
conn-secondary-cert = Secondary client certificate (MCA, optional)
conn-secondary-key = Secondary private key (optional)
conn-secondary-password = Secondary key password
conn-software-token = Software token
conn-token-string = Token string
conn-ca-cert-path = CA certificate path
conn-split-mode = Split tunnel mode
conn-split-networks = Custom split networks
conn-reported-os = Reported OS
conn-client-version = Client version
conn-http-proxy = HTTP proxy
conn-cert-pin = Server cert pin
conn-trusted-apps = Trusted app packages
conn-blocked-apps = Blocked app packages
conn-dtls = Enable DTLS data path (recommended)
conn-pfs = Require perfect forward secrecy
conn-no-xml-post = Disable XML POST (rare gateways)
conn-reject-mismatch = Reject hostname mismatch or incomplete chains
conn-abort-untrusted = Abort when the server is untrusted
conn-allow-insecure = Allow insecure cryptography
conn-allow-insecure-desc = Only for legacy 3DES/RC4/SHA1 gateways; independent of certificate trust
conn-local-lan = Keep local network reachable while connected
conn-auto-connect = Bring the tunnel up when the network is available
conn-fips-unavailable = The current runtime has no validated FIPS provider

# --- Logs ---
logs-current-tab = Current
logs-history-tab = History
logs-recording-active-detail = Recording; stop before deleting
logs-recording-on = Recording and saving daily
logs-recording-off = Log recording is off
logs-files-suffix = log files
logs-no-history = No log history
logs-no-history-desc = Daily files appear after recording is enabled
logs-count-suffix = logs
logs-tap-detail = Tap a log for details
logs-start-recording-hint = Tap the top-right button to start recording
logs-delete-title = Delete log history?
logs-delete-desc = This cannot be undone
logs-delete-action = Delete
logs-detail-title = Log details
