import { common } from '@kit.AbilityKit';
import { util } from '@kit.ArkTS';
import { fileIo, picker } from '@kit.CoreFileKit';
import { hilog } from '@kit.PerformanceAnalysisKit';
import { describeError } from '../common/Errors';

const LOG_DOMAIN: number = 0x0000;
const LOG_TAG: string = 'HOpenConnectExport';

/**
 * Exports diagnostic log archives and connection QR PNG images through the
 * system document picker with pre-filled suggested filenames.
 */
export class ExportService {
  private readonly context?: common.UIAbilityContext;

  constructor(context?: common.UIAbilityContext) {
    this.context = context;
  }

  private createPicker(): picker.DocumentViewPicker {
    return this.context
      ? new picker.DocumentViewPicker(this.context)
      : new picker.DocumentViewPicker();
  }

  async exportLog(suggestedName: string, content: string): Promise<void> {
    if (!content || content.length === 0) {
      throw new Error('log export content is empty');
    }
    const baseName = (suggestedName || 'h-openconnect-log')
      .replace(/[\\/:*?"<>|]/g, '_')
      .trim() || 'h-openconnect-log';
    const fileName = baseName.endsWith('.log') ? baseName : `${baseName}.log`;
    const documentPicker = this.createPicker();
    const options = new picker.DocumentSaveOptions();
    options.newFileNames = [fileName];
    options.fileSuffixChoices = ['Log|.log', 'Text|.txt'];
    const result: Array<string> = await documentPicker.save(options);
    if (!result || result.length === 0 || result[0].length === 0) {
      throw new Error('log export cancelled');
    }
    let file: fileIo.File | undefined = undefined;
    try {
      file = fileIo.openSync(
        result[0],
        fileIo.OpenMode.WRITE_ONLY | fileIo.OpenMode.CREATE | fileIo.OpenMode.TRUNC,
      );
      fileIo.writeSync(file.fd, content);
      hilog.info(LOG_DOMAIN, LOG_TAG, 'exported log archive');
    } catch (err) {
      throw new Error(describeError(err) || 'write exported log failed');
    } finally {
      if (file) {
        try {
          fileIo.closeSync(file);
        } catch (err) {
          hilog.error(LOG_DOMAIN, LOG_TAG, 'close exported log failed %{public}s', describeError(err));
        }
      }
    }
  }

  async exportImage(suggestedName: string, pngBase64: string): Promise<void> {
    const bytes = new util.Base64Helper().decodeSync(pngBase64);
    if (bytes.byteLength < 8 || bytes[0] !== 0x89 || bytes[1] !== 0x50 ||
      bytes[2] !== 0x4e || bytes[3] !== 0x47) {
      throw new Error('connection QR image is not a PNG');
    }
    const baseName = (suggestedName || 'connection')
      .replace(/[\\/:*?"<>|]/g, '_')
      .trim() || 'connection';
    const fileName = baseName.endsWith('.png') ? baseName : `${baseName}.png`;
    const documentPicker = this.createPicker();
    const options = new picker.DocumentSaveOptions();
    options.newFileNames = [fileName];
    options.fileSuffixChoices = ['PNG|.png'];
    const result: Array<string> = await documentPicker.save(options);
    if (!result || result.length === 0 || result[0].length === 0) {
      throw new Error('connection QR export cancelled');
    }
    let file: fileIo.File | undefined = undefined;
    try {
      file = fileIo.openSync(
        result[0],
        fileIo.OpenMode.WRITE_ONLY | fileIo.OpenMode.CREATE | fileIo.OpenMode.TRUNC,
      );
      const written = fileIo.writeSync(file.fd, bytes.slice().buffer as ArrayBuffer);
      if (written !== bytes.byteLength) {
        throw new Error(`incomplete connection QR image write (${written}/${bytes.byteLength})`);
      }
      hilog.info(LOG_DOMAIN, LOG_TAG, 'exported connection QR image');
    } catch (err) {
      throw new Error(describeError(err) || 'write exported connection QR image failed');
    } finally {
      if (file) {
        try {
          fileIo.closeSync(file);
        } catch (err) {
          hilog.error(LOG_DOMAIN, LOG_TAG, 'close exported image failed %{public}s', describeError(err));
        }
      }
    }
  }
}
