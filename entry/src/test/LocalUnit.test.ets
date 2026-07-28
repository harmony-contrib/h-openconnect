import { describe, it, expect } from '@ohos/hypium';

export default function localUnitTest() {
  describe('localUnitTest', () => {
    it('assertEqual', 0, () => {
      expect('H-AnyConnect').assertEqual('H-AnyConnect');
    });
  });
}
