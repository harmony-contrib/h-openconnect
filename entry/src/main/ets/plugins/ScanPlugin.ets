import { AsyncPluginBase, BridgeCallContext, BridgeContextRequirement, BridgeTypedValue } from '@ohos-rs/ability';
import { scanBarcode, scanCore } from '@kit.ScanKit';
import { describeError } from '../common/Errors';

const SCAN_CANCELLED_CODE: number = 1000500002;
const SCAN_REQUEST_TYPE = 'hopenconnect.ScanRequest';
const SCAN_RESPONSE_TYPE = 'hopenconnect.ScanResponse';

interface ScanErrorLike {
  code?: number;
}

/** Return the scanned QR payload to Rust; an empty value means cancellation. */
export class ScanPlugin extends AsyncPluginBase {
  readonly id = 'hopenconnect.scan';
  readonly requires: BridgeContextRequirement[] = ['ability'];

  async invokeAsync(
    action: string,
    request: BridgeTypedValue,
    context: BridgeCallContext,
  ): Promise<BridgeTypedValue> {
    if (action !== 'scan-qr') {
      throw new Error(`Unsupported hopenconnect.scan action '${action}'`);
    }
    if (request.typeName !== SCAN_REQUEST_TYPE || typeof request.value !== 'object') {
      throw new Error(`hopenconnect.scan expected ${SCAN_REQUEST_TYPE}, got ${request.typeName}`);
    }
    const options: scanBarcode.ScanOptions = {
      scanTypes: [scanCore.ScanType.QR_CODE],
      enableMultiMode: false,
      enableAlbum: true,
    };
    try {
      const result: scanBarcode.ScanResult =
        await scanBarcode.startScanForResult(context.abilityContext, options);
      return { typeName: SCAN_RESPONSE_TYPE, value: { content: result.originalValue.trim() } };
    } catch (err) {
      const error = err as ScanErrorLike;
      if (error.code === SCAN_CANCELLED_CODE) {
        return { typeName: SCAN_RESPONSE_TYPE, value: { content: '' } };
      }
      throw new Error(describeError(err) || 'scan connection failed');
    }
  }
}
