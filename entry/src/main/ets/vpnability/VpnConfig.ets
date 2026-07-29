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

export const HANY_BUNDLE_NAME: string = 'com.southorange.hanyconnect';
export const HANY_MODULE_NAME: string = 'entry';
export const HANY_ENTRY_ABILITY: string = 'EntryAbility';
export const HANY_VPN_ABILITY: string = 'HAnyConnectVpnExtensionAbility';

let hanyBundleName: string = HANY_BUNDLE_NAME;

export function setHAnyBundleName(bundleName?: string): void {
  const value = bundleName?.trim() ?? '';
  hanyBundleName = value.length > 0 ? value : HANY_BUNDLE_NAME;
}

export function currentHAnyBundleName(): string {
  return hanyBundleName;
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

export class HAnyVpnConfig {
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
  return new HAnyVpnConfig(options) as vpnExtension.VpnConfig;
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

export function readOptionsJson(want?: Want): string {
  const parameters = want?.parameters as Record<string, Object> | undefined;
  const value = parameters?.['optionsJson'];
  if (typeof value === 'string' && value.length > 0) {
    return value;
  }
  throw new Error('VPN extension request is missing optionsJson');
}

const HANY_ASHMEM_FD_KEY = 'hanyconnectPlatformAshmem';
const HANY_NOTIFICATION_FD_KEY = 'hanyconnectPlatformNotification';

export interface HAnyPlatformSharedMemoryFds {
  ashmemFd: number;
  notificationFd: number;
}

class HAnyWantFileDescriptor {
  public type: string = 'FD';
  public value: number;

  constructor(value: number) {
    this.value = value;
  }
}

interface HAnyVpnWantParameters extends Record<string, Object> {
  optionsJson: string;
}

export function buildVpnWant(
  optionsJson?: string,
  sharedMemory?: HAnyPlatformSharedMemoryFds,
): Want {
  const parameters: HAnyVpnWantParameters = {
    optionsJson: optionsJson ?? '',
  };
  if (sharedMemory) {
    parameters[HANY_ASHMEM_FD_KEY] = new HAnyWantFileDescriptor(sharedMemory.ashmemFd);
    parameters[HANY_NOTIFICATION_FD_KEY] = new HAnyWantFileDescriptor(sharedMemory.notificationFd);
  }
  return {
    deviceId: '',
    bundleName: currentHAnyBundleName(),
    moduleName: HANY_MODULE_NAME,
    abilityName: HANY_VPN_ABILITY,
    parameters,
  };
}

export function readPlatformSharedMemoryFds(want: Want): HAnyPlatformSharedMemoryFds | undefined {
  const ashmemFd = want.fds?.[HANY_ASHMEM_FD_KEY];
  const notificationFd = want.fds?.[HANY_NOTIFICATION_FD_KEY];
  if (typeof ashmemFd !== 'number' || ashmemFd < 0 ||
    typeof notificationFd !== 'number' || notificationFd < 0) {
    return undefined;
  }
  return { ashmemFd, notificationFd };
}
