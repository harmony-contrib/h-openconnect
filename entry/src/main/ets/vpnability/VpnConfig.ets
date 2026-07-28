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

const DEFAULT_OPTIONS: VpnOptions = {
  addresses: ['10.0.0.2/30'],
  routes: ['0.0.0.0/0'],
  dnsAddresses: ['1.1.1.1', '8.8.8.8'],
  mtu: 1400,
  allowBypass: false,
};

export const DEFAULT_OPTIONS_JSON: string = JSON.stringify(DEFAULT_OPTIONS);

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
    const rawAddrs = options.addresses?.length ? options.addresses : ['10.0.0.2/30'];
    const dnsList = options.dnsAddresses?.slice() ?? [];

    this.addresses = rawAddrs.map((value: string) =>
      parseLinkAddress(value, addressDefaultPrefix(value)));

    const routeStrs = options.routes?.length ? options.routes : ['0.0.0.0/0'];
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
    return DEFAULT_OPTIONS;
  }
  try {
    return JSON.parse(optionsJson) as VpnOptions;
  } catch (_err) {
    return DEFAULT_OPTIONS;
  }
}

export function readOptionsJson(want?: Want): string {
  const parameters = want?.parameters as Record<string, Object> | undefined;
  const value = parameters?.['optionsJson'];
  if (typeof value === 'string' && value.length > 0) {
    return value;
  }
  return DEFAULT_OPTIONS_JSON;
}

export function buildVpnWant(optionsJson?: string): Want {
  return {
    deviceId: '',
    bundleName: currentHAnyBundleName(),
    moduleName: HANY_MODULE_NAME,
    abilityName: HANY_VPN_ABILITY,
    parameters: {
      optionsJson: optionsJson ?? DEFAULT_OPTIONS_JSON,
    },
  };
}
