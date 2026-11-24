export type ConnectionLifecycle =
  | "disconnected"
  | "connecting"
  | "authenticating"
  | "establishing"
  | "connected"
  | "disconnecting"
  | "failed";

export type ProtocolKind =
  | "anyConnect"
  | "ssl"
  | "ipsec"
  | "juniper"
  | "globalProtect"
  | "pulse"
  | "f5"
  | "fortinet"
  | "array";

export type AuthMethod =
  | "password"
  | "certificate"
  | "passwordAndCertificate"
  | "saml";

export type SoftwareToken = "disabled" | "securId" | "totp";

export type SplitTunnelMode = "auto" | "onVpnDns" | "onUplinkDns";

export type LanguagePreference = "system" | "zh-CN" | "en";
export type ThemePreference = "system" | "light" | "dark";

export interface ConnectionProfile {
  id: string;
  name: string;
  server: string;
  group: string;
  username: string;
  password: string;
  protocol: ProtocolKind;
  authMethod: AuthMethod;
  certificate: string;
  privateKey: string;
  secondaryCertificate: string;
  secondaryPrivateKey: string;
  caCertificate: string;
  keyPassword: string;
  secondaryKeyPassword: string;
  httpProxy: string;
  serverCertHash: string;
  backupServers: string;
  strictCertificateTrust: boolean;
  blockUntrustedServers: boolean;
  allowLocalLan: boolean;
  forceGlobal: boolean;
  splitTunnelMode: SplitTunnelMode;
  splitTunnelNetworks: string;
  connectOnDemand: boolean;
  externalBrowserAuth: boolean;
  fipsMode: boolean;
  allowInsecureCrypto: boolean;
  useDtls: boolean;
  reportedOs: string;
  userAgent: string;
  clientVersion: string;
  sni: string;
  requirePfs: boolean;
  disableXmlPost: boolean;
  dpdSeconds: number;
  softwareToken: SoftwareToken;
  tokenString: string;
  csdWrapper: string;
  trustedApplications: string;
  blockedApplications: string;
  mtu: number;
  favorite: boolean;
}

export function newDraftProfile(): ConnectionProfile {
  return {
    id: "",
    name: "",
    server: "",
    group: "",
    username: "",
    password: "",
    protocol: "anyConnect",
    authMethod: "password",
    certificate: "",
    privateKey: "",
    secondaryCertificate: "",
    secondaryPrivateKey: "",
    caCertificate: "",
    keyPassword: "",
    secondaryKeyPassword: "",
    httpProxy: "",
    serverCertHash: "",
    backupServers: "",
    strictCertificateTrust: true,
    blockUntrustedServers: true,
    allowLocalLan: false,
    forceGlobal: false,
    splitTunnelMode: "auto",
    splitTunnelNetworks: "",
    connectOnDemand: false,
    externalBrowserAuth: false,
    fipsMode: false,
    allowInsecureCrypto: false,
    useDtls: true,
    reportedOs: "OpenHarmony",
    userAgent: "",
    clientVersion: "",
    sni: "",
    requirePfs: false,
    disableXmlPost: false,
    dpdSeconds: 0,
    softwareToken: "disabled",
    tokenString: "",
    csdWrapper: "",
    trustedApplications: "",
    blockedApplications: "",
    mtu: 0,
    favorite: false,
  };
}

export interface SessionStats {
  bytesSent: number;
  bytesReceived: number;
  packetsSent: number;
  packetsReceived: number;
  connectedSeconds: number;
  assignedIp: string;
  gateway: string;
  mtu: number;
}

export interface NetworkSnapshot {
  address?: string | null;
  netmask?: string | null;
  addressV6?: string | null;
  netmaskV6?: string | null;
  gateway?: string | null;
  dns: string[];
  mtu: number;
  routes: string[];
  splitExcludes: string[];
  domain?: string | null;
  splitDns: string[];
}

export interface TrafficSample {
  rxBytesPerSec: number;
  txBytesPerSec: number;
}

export interface AuthFieldKey {
  formId?: string | null;
  optionIndex: number;
  optionDigest: string;
}

export interface AuthFieldChoice {
  name: string;
  label: string;
}

export type AuthFieldKind =
  | "text"
  | "password"
  | "select"
  | "token"
  | "hidden"
  | "unknown";

export interface AuthField {
  key: AuthFieldKey;
  name: string;
  label: string;
  kind: AuthFieldKind;
  value: string;
  choices: AuthFieldChoice[];
  authGroup: boolean;
  secondAuth: boolean;
  required: boolean;
}

export interface AuthChallenge {
  id: number;
  round: number;
  banner?: string | null;
  message?: string | null;
  error?: string | null;
  formId?: string | null;
  method?: string | null;
  fields: AuthField[];
}

export interface AuthFieldValue {
  key: AuthFieldKey;
  value: string;
}

export interface AuthGroupDiscovery {
  selected?: string | null;
  groups: AuthFieldChoice[];
}

export interface DiagnosticEntry {
  level: string;
  message: string;
  timestamp: string;
}

export interface LogArchiveSummary {
  fileName: string;
  date: string;
  bytes: number;
  updatedAt: string;
  active: boolean;
}

export interface LogRecordingStatus {
  enabled: boolean;
  activeFile?: string | null;
  archives: LogArchiveSummary[];
}

export type RuntimeDomain =
  | "config"
  | "status"
  | "profiles"
  | "statistics"
  | "logs"
  | "about";

export interface ConfigDomainData {
  activeConnectionId?: string | null;
  language: LanguagePreference;
  theme: ThemePreference;
  systemLocale: string;
  systemColorMode: number;
  sdkReady: boolean;
  backend: string;
}

export interface StatusDomainData {
  lifecycle: ConnectionLifecycle;
  vpnSessionId: string;
  lastError?: string | null;
  sdkReady: boolean;
  backend: string;
  pendingAuth?: AuthChallenge | null;
  operationPending: boolean;
}

export interface ProfilesDomainData {
  activeConnectionId?: string | null;
  profiles: ConnectionProfile[];
}

export interface StatisticsDomainData {
  lifecycle: ConnectionLifecycle;
  stats: SessionStats;
  network: NetworkSnapshot;
  trafficHistory: TrafficSample[];
}

export interface LogsDomainData {
  entries: DiagnosticEntry[];
  recording: LogRecordingStatus;
}

export interface AboutDomainData {
  appVersion: string;
  openconnectVersion?: string | null;
  backend: string;
  sdkReady: boolean;
}

export interface DomainDataMap {
  config: ConfigDomainData;
  status: StatusDomainData;
  profiles: ProfilesDomainData;
  statistics: StatisticsDomainData;
  logs: LogsDomainData;
  about: AboutDomainData;
}

export interface PreferencesSnapshot {
  language: LanguagePreference;
  theme: ThemePreference;
  systemLocale: string;
  systemColorMode: number;
}

export interface DomainRevision {
  domain: RuntimeDomain;
  revision: number;
}

export interface StateChanged {
  generation: number;
  revision: number;
  changes: DomainRevision[];
}

export interface StateBarrier {
  generation: number;
  required: DomainRevision[];
}

export interface DomainProjection<D extends RuntimeDomain = RuntimeDomain> {
  generation: number;
  domain: D;
  revision: number;
  data: DomainDataMap[D];
}

export interface CommandResult<T = unknown> {
  id: number;
  ok: boolean;
  data?: T;
  error?: string;
  barrier?: StateBarrier;
}

export function isLifecycleBusy(lifecycle: ConnectionLifecycle): boolean {
  return (
    lifecycle === "connecting" ||
    lifecycle === "authenticating" ||
    lifecycle === "establishing" ||
    lifecycle === "disconnecting"
  );
}

export function isLifecycleConnected(lifecycle: ConnectionLifecycle): boolean {
  return lifecycle === "connected";
}

export function fieldKeyString(key: AuthFieldKey): string {
  return `${key.formId ?? ""}:${key.optionIndex}:${key.optionDigest}`;
}

export function normalizeProfile(raw: Partial<ConnectionProfile>): ConnectionProfile {
  return { ...newDraftProfile(), ...raw };
}
