/**
 * Direct AnyConnect/OpenConnect -> HarmonyOS VpnConfig mapping.
 *
 * The native layer has already applied the same policy as ics-openconnect:
 * the headend address/netmask, split-include routes, split-excludes, DNS
 * host routes and search suffixes are authoritative. Keep this class as a
 * shape adapter only; changing prefixes or synthesising routes here changes
 * the policy selected by the AnyConnect group.
 */
import { Want } from '@kit.AbilityKit';
import { vpnExtension } from '@kit.NetworkKit';

export const HOPEN_BUNDLE_NAME: string = 'com.richerfu.h_openconnect';
export const HOPEN_MODULE_NAME: string = 'entry';
export const HOPEN_ENTRY_ABILITY: string = 'EntryAbility';
export const HOPEN_VPN_ABILITY: string = 'HOpenConnectVpnExtensionAbility';

let hopenBundleName: string = HOPEN_BUNDLE_NAME;

export function setHOpenBundleName(bundleName?: string): void {
  const value = bundleName?.trim() ?? '';
  hopenBundleName = value.length > 0 ? value : HOPEN_BUNDLE_NAME;
}

export function currentHOpenBundleName(): string {
  return hopenBundleName;
}

export class NetAddress {
  public address: string;
  public family: number;
  public port: number;

  constructor(address: string, family: number, port: number = 0) {
    this.address = address;
    this.family = family;
    this.port = port;
  }
}

export class LinkAddress {
  public address: NetAddress;
  public prefixLength: number;

  constructor(address: NetAddress, prefixLength: number) {
    this.address = address;
    this.prefixLength = prefixLength;
  }
}

/** paws RouteInfo — fixed iface name for classic vpn-tun. */
export class RouteInfo {
  public interface: string;
  public destination: LinkAddress;
  public gateway: NetAddress;
  public hasGateway: boolean;
  public isDefaultRoute: boolean;
  public isExcludedRoute: boolean;

  constructor(destination: LinkAddress, isExcludedRoute: boolean = false) {
    this.interface = 'vpn-tun';
    this.destination = destination;
    this.gateway = new NetAddress(
      destination.address.family === 2 ? '::' : '0.0.0.0',
      destination.address.family,
    );
    this.hasGateway = false;
    this.isDefaultRoute = destination.prefixLength === 0 && !isExcludedRoute;
    this.isExcludedRoute = isExcludedRoute;
  }
}

export interface VpnOptions {
  addresses?: string[];
  routes?: string[];
  excludedRoutes?: string[];
  dnsAddresses?: string[];
  searchDomains?: string[];
  mtu?: number;
  allowBypass?: boolean;
  forceGlobal?: boolean;
  trustedApplications?: string[];
  blockedApplications?: string[];
}

export class HOpenVpnConfig {
  public addresses: LinkAddress[];
  public routes: RouteInfo[];
  public dnsAddresses: string[];
  public searchDomains: string[];
  public mtu: number;
  public isIPv4Accepted: boolean;
  public isIPv6Accepted: boolean;
  public isBlocking: boolean;
  public trustedApplications: string[];
  public blockedApplications: string[];

  constructor(options: VpnOptions) {
    const rawAddrs = options.addresses ?? [];
    if (rawAddrs.length === 0) {
      throw new Error('VPN configuration has no assigned address');
    }
    const dnsList = options.dnsAddresses?.slice() ?? [];

    this.addresses = rawAddrs.map((value: string) =>
      parseLinkAddress(value, addressDefaultPrefix(value)));

    const routeStrs = options.routes ?? [];
    this.routes = routeStrs.map((value: string) =>
      new RouteInfo(parseLinkAddress(value, routeDefaultPrefix(value))));
    const excludedRouteStrs = options.excludedRoutes ?? [];
    this.routes.push(...excludedRouteStrs.map((value: string) =>
      new RouteInfo(parseLinkAddress(value, routeDefaultPrefix(value)), true)));

    this.dnsAddresses = dnsList;
    this.searchDomains = options.searchDomains?.slice() ?? [];
    let mtu = (options.mtu ?? 1400) > 0 ? (options.mtu ?? 1400) : 1400;
    if (mtu < 576) {
      mtu = 576;
    }
    if (mtu > 1500) {
      mtu = 1500;
    }
    this.mtu = mtu;
    this.isIPv4Accepted = this.addresses.some((address: LinkAddress) =>
      address.address.family === 1);
    this.isIPv6Accepted = this.addresses.some((address: LinkAddress) =>
      address.address.family === 2);
    this.isBlocking = false;
    this.trustedApplications = options.trustedApplications?.slice() ?? [];
    this.blockedApplications = options.blockedApplications?.slice() ?? [];
  }
}

/** paws parseLinkAddress. */
export function parseLinkAddress(value: string, defaultPrefix: number): LinkAddress {
  const parts = value.split('/');
  const address = parts[0];
  const prefixLength = parts.length > 1 ? Number.parseInt(parts[1], 10) : defaultPrefix;
  const family = address.includes(':') ? 2 : 1;
  return new LinkAddress(
    new NetAddress(address, family),
    Number.isFinite(prefixLength) ? prefixLength : defaultPrefix,
  );
}

export function addressDefaultPrefix(value: string): number {
  return value.includes(':') ? 128 : 32;
}

export function routeDefaultPrefix(value: string): number {
  return value.includes(':') ? 128 : 32;
}

export function buildVpnConfig(options: VpnOptions, _vpnId?: string): vpnExtension.VpnConfig {
  return new HOpenVpnConfig(options) as vpnExtension.VpnConfig;
}

export function parseOptions(optionsJson?: string): VpnOptions {
  if (!optionsJson || optionsJson.length === 0) {
    throw new Error('VPN options are missing');
  }
  try {
    const options = JSON.parse(optionsJson) as VpnOptions;
    if (!options || !Array.isArray(options.addresses) || options.addresses.length === 0) {
      throw new Error('VPN options contain no assigned address');
    }
    if (options.routes !== undefined && !Array.isArray(options.routes)) {
      throw new Error('VPN routes must be an array');
    }
    if (options.excludedRoutes !== undefined && !Array.isArray(options.excludedRoutes)) {
      throw new Error('VPN excluded routes must be an array');
    }
    if (options.dnsAddresses !== undefined && !Array.isArray(options.dnsAddresses)) {
      throw new Error('VPN DNS addresses must be an array');
    }
    return options;
  } catch (err) {
    const message = err instanceof Error ? err.message : 'invalid VPN options JSON';
    throw new Error(`invalid VPN options: ${message}`);
  }
}

const HOPEN_SYSTEM_PARAMETERS_KEY = 'myParams';

/**
 * The VPN authorization dialog preserves the original WantParams under
 * `myParams` before it starts the extension for the first time. This shape is
 * part of the API 24 VPN authorization path. Later starts deliver the original
 * parameters directly, so accept both forms without retrying the ability.
 */
function readVpnParameters(want?: Want): Record<string, Object> | undefined {
  const parameters = want?.parameters as Record<string, Object> | undefined;
  if (!parameters) {
    return undefined;
  }
  if (typeof parameters['optionsJson'] === 'string') {
    return parameters;
  }
  const nested = parameters[HOPEN_SYSTEM_PARAMETERS_KEY];
  if (typeof nested === 'object' && nested !== null && !Array.isArray(nested)) {
    return nested as Record<string, Object>;
  }
  return parameters;
}

export function readOptionsJson(want?: Want): string {
  const parameters = readVpnParameters(want);
  const value = parameters?.['optionsJson'];
  if (typeof value === 'string' && value.length > 0) {
    return value;
  }
  throw new Error('VPN extension request is missing optionsJson');
}

const HOPEN_ASHMEM_FD_KEY = 'hopenconnectPlatformAshmem';
const HOPEN_NOTIFICATION_FD_KEY = 'hopenconnectPlatformNotification';
const HOPEN_START_ATTEMPT_ID_KEY = 'hopenconnectStartAttemptId';

export interface HOpenPlatformSharedMemoryFds {
  ashmemFd: number;
  notificationFd: number;
}

class HOpenWantFileDescriptor {
  public type: string = 'FD';
  public value: number;

  constructor(value: number) {
    this.value = value;
  }
}

interface HOpenVpnWantParameters extends Record<string, Object> {
  optionsJson: string;
}

export function buildVpnWant(
  optionsJson?: string,
  sharedMemory?: HOpenPlatformSharedMemoryFds,
  startAttemptId?: string,
): Want {
  const parameters: HOpenVpnWantParameters = {
    optionsJson: optionsJson ?? '',
  };
  if (sharedMemory) {
    parameters[HOPEN_ASHMEM_FD_KEY] = new HOpenWantFileDescriptor(sharedMemory.ashmemFd);
    parameters[HOPEN_NOTIFICATION_FD_KEY] = new HOpenWantFileDescriptor(sharedMemory.notificationFd);
  }
  if (startAttemptId && startAttemptId.length > 0) {
    parameters[HOPEN_START_ATTEMPT_ID_KEY] = startAttemptId;
  }
  return {
    deviceId: '',
    bundleName: currentHOpenBundleName(),
    moduleName: HOPEN_MODULE_NAME,
    abilityName: HOPEN_VPN_ABILITY,
    parameters,
  };
}

export function readPlatformStartAttemptId(want?: Want): string {
  const value = readVpnParameters(want)?.[HOPEN_START_ATTEMPT_ID_KEY];
  if (typeof value === 'string' && value.length > 0) {
    return value;
  }
  throw new Error('VPN extension request is missing start attempt id');
}

export function readPlatformSharedMemoryFds(want: Want): HOpenPlatformSharedMemoryFds | undefined {
  let ashmemFd = want.fds?.[HOPEN_ASHMEM_FD_KEY];
  let notificationFd = want.fds?.[HOPEN_NOTIFICATION_FD_KEY];
  if (!isValidFileDescriptor(ashmemFd) || !isValidFileDescriptor(notificationFd)) {
    const parameters = readVpnParameters(want);
    ashmemFd = readFileDescriptorParameter(parameters, HOPEN_ASHMEM_FD_KEY);
    notificationFd = readFileDescriptorParameter(parameters, HOPEN_NOTIFICATION_FD_KEY);
  }
  if (!isValidFileDescriptor(ashmemFd) || !isValidFileDescriptor(notificationFd)) {
    return undefined;
  }
  return {
    ashmemFd: ashmemFd as number,
    notificationFd: notificationFd as number,
  };
}

function readFileDescriptorParameter(
  parameters: Record<string, Object> | undefined,
  key: string,
): number | undefined {
  const descriptor = parameters?.[key] as HOpenWantFileDescriptor | undefined;
  if (!descriptor || descriptor.type !== 'FD' || !isValidFileDescriptor(descriptor.value)) {
    return undefined;
  }
  return descriptor.value;
}

function isValidFileDescriptor(value?: number): boolean {
  return typeof value === 'number' && Number.isInteger(value) && value >= 0;
}
