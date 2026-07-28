import { describe, it, expect } from '@ohos/hypium';

export default function abilityTest() {
  describe('ActsAbilityTest', () => {
    it('assertContain', 0, () => {
      const a = 'abc';
      const b = 'b';
      expect(a).assertContain(b);
      expect(a).assertEqual(a);
    });
  });
}
