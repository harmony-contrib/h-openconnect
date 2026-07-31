import { describe, it, expect } from '@ohos/hypium';
import {
  buildVpnConfig,
  readOptionsJson,
  readPlatformSharedMemoryFds,
  readPlatformStartAttemptId,
  parseOptions,
} from '../main/ets/vpnability/VpnConfig';
import { Want } from '@kit.AbilityKit';

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

    it('readsDirectVpnExtensionWant', 0, () => {
      const want = {
        parameters: {
          optionsJson: '{"addresses":["10.0.0.2/24"]}',
          hopenconnectStartAttemptId: 'attempt-direct',
        },
        fds: {
          hopenconnectPlatformAshmem: 21,
          hopenconnectPlatformNotification: 22,
        },
      } as Want;

      const sharedMemory = readPlatformSharedMemoryFds(want);
      expect(readOptionsJson(want)).assertEqual('{"addresses":["10.0.0.2/24"]}');
      expect(readPlatformStartAttemptId(want)).assertEqual('attempt-direct');
      expect(sharedMemory?.ashmemFd).assertEqual(21);
      expect(sharedMemory?.notificationFd).assertEqual(22);
    });

    it('readsFirstAuthorizationVpnExtensionWant', 0, () => {
      const want = {
        parameters: {
          callingUid: 20010048,
          callingPid: 1234,
          myParams: {
            optionsJson: '{"addresses":["10.0.0.2/24"]}',
            hopenconnectStartAttemptId: 'attempt-first-authorization',
            hopenconnectPlatformAshmem: {
              type: 'FD',
              value: 31,
            },
            hopenconnectPlatformNotification: {
              type: 'FD',
              value: 32,
            },
          },
        },
      } as Want;

      const sharedMemory = readPlatformSharedMemoryFds(want);
      expect(readOptionsJson(want)).assertEqual('{"addresses":["10.0.0.2/24"]}');
      expect(readPlatformStartAttemptId(want)).assertEqual('attempt-first-authorization');
      expect(sharedMemory?.ashmemFd).assertEqual(31);
      expect(sharedMemory?.notificationFd).assertEqual(32);
    });

    it('rejectsMalformedNestedFileDescriptors', 0, () => {
      const want = {
        parameters: {
          myParams: {
            optionsJson: '{"addresses":["10.0.0.2/24"]}',
            hopenconnectPlatformAshmem: {
              type: 'not-an-fd',
              value: 31,
            },
            hopenconnectPlatformNotification: {
              type: 'FD',
              value: -1,
            },
          },
        },
      } as Want;

      expect(readPlatformSharedMemoryFds(want) === undefined).assertEqual(true);
    });
  });
}
