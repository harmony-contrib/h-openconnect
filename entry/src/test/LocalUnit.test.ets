import { describe, it, expect } from '@ohos/hypium';
import {
  buildVpnConfig,
  parseOptions,
} from '../main/ets/vpnability/VpnConfig';

export default function localUnitTest() {
  describe('VpnConfig', () => {
    it('mapsHeadendConfigurationWithoutSynthesis', 0, () => {
      const options = parseOptions(JSON.stringify({
        addresses: ['11.36.24.95/26'],
        routes: ['11.36.24.64/26'],
        excludedRoutes: ['192.168.3.0/24'],
        dnsAddresses: ['11.1.1.53'],
        searchDomains: ['corp.example'],
        mtu: 1309,
      }));
      const config = buildVpnConfig(options);

      expect(config.addresses?.length).assertEqual(1);
      expect(config.routes?.length).assertEqual(2);
      expect(config.dnsAddresses?.length).assertEqual(1);
      expect(config.mtu).assertEqual(1309);
    });

    it('rejectsMissingAssignedAddress', 0, () => {
      let rejected = false;
      try {
        parseOptions(JSON.stringify({
          routes: ['0.0.0.0/0'],
          dnsAddresses: ['8.8.8.8'],
        }));
      } catch (_err) {
        rejected = true;
      }
      expect(rejected).assertEqual(true);
    });

    it('rejectsMalformedOptionsInsteadOfUsingDefaults', 0, () => {
      let rejected = false;
      try {
        parseOptions('{not-json');
      } catch (_err) {
        rejected = true;
      }
      expect(rejected).assertEqual(true);
    });
  });
}
