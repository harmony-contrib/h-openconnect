import { common } from '@kit.AbilityKit';
import { util } from '@kit.ArkTS';
import { image } from '@kit.ImageKit';
import { generateBarcode, scanBarcode, scanCore } from '@kit.ScanKit';
import { describeError } from '../common/Errors';

const SCAN_CANCELLED_CODE: number = 1000500002;
const MAX_QR_BYTES: number = 2953;
const LEVEL_M_MAX_BYTES: number = 1536;

interface ScanErrorLike {
  code?: number;
}

export interface QrImageResult {
  source: string;
  pngBase64: string;
}

/**
 * Generates PNG QR codes and scans QR codes via HarmonyOS ScanKit.
 */
export class QrService {
  async generateQr(text: string): Promise<QrImageResult> {
    if (!text || text.length === 0) {
      throw new Error('QR payload is empty');
    }
    const encoder = util.TextEncoder.create('utf-8');
    const encoded: Uint8Array = encoder.encodeInto(text);
    if (!encoded || encoded.byteLength === 0 || encoded.byteLength > MAX_QR_BYTES) {
      throw new Error('Connection configuration is too large for a QR code');
    }
    const contentBuffer: ArrayBuffer = encoded.buffer.slice(
      encoded.byteOffset,
      encoded.byteOffset + encoded.byteLength,
    );
    const options: generateBarcode.CreateOptions = {
      scanType: scanCore.ScanType.QR_CODE,
      width: 1024,
      height: 1024,
      margin: 4,
      level: encoded.byteLength > LEVEL_M_MAX_BYTES
        ? generateBarcode.ErrorCorrectionLevel.LEVEL_L
        : generateBarcode.ErrorCorrectionLevel.LEVEL_M,
    };
    // Use the ArrayBuffer overload: ScanKit caps createBarcode(string, ...) at
    // 512 characters (error 401), whereas serialized connection profiles are
    // ~1.1 KB and fit comfortably in the byte-array overload.
    const bitmap: image.PixelMap = await generateBarcode.createBarcode(contentBuffer, options);
    try {
      const packer = image.createImagePacker();
      try {
        const packing: image.PackingOption = { format: 'image/png', quality: 100 };
        const png: ArrayBuffer = await packer.packToData(bitmap, packing);
        const pngBase64 = new util.Base64Helper().encodeToStringSync(new Uint8Array(png));
        return {
          source: `data:image/png;base64,${pngBase64}`,
          pngBase64,
        };
      } finally {
        await packer.release();
      }
    } finally {
      await bitmap.release();
    }
  }

  async scanQr(context: common.UIAbilityContext): Promise<string> {
    const options: scanBarcode.ScanOptions = {
      scanTypes: [scanCore.ScanType.QR_CODE],
      enableMultiMode: false,
      enableAlbum: true,
    };
    try {
      const result: scanBarcode.ScanResult = await scanBarcode.startScanForResult(context, options);
      return (result.originalValue ?? '').trim();
    } catch (err) {
      const error = err as ScanErrorLike;
      if (error.code === SCAN_CANCELLED_CODE) {
        return '';
      }
      throw new Error(describeError(err) || 'scan QR code failed');
    }
  }
}
