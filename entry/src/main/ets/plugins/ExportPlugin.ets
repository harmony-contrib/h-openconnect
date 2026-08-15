import { AsyncPluginBase, BridgeCallContext, BridgeContextRequirement, BridgeTypedValue } from '@ohos-rs/ability';
import { fileIo, picker } from '@kit.CoreFileKit';
import { hilog } from '@kit.PerformanceAnalysisKit';
import { describeError } from '../common/Errors';

const LOG_DOMAIN: number = 0x0000;
const LOG_TAG: string = 'HOpenConnectExport';

const EXPORT_REQUEST_TYPE = 'hopenconnect.ExportTextRequest';
const EXPORT_RESPONSE_TYPE = 'hopenconnect.ExportTextResponse';

interface ExportTextRequest {
  suggestedName: string;
  content: string;
}

/**
 * `hopenconnect.export` — exports a log archive through the system document
 * picker with a pre-filled suggested file name.
 */
export class ExportPlugin extends AsyncPluginBase {
  readonly id = 'hopenconnect.export';
  readonly requires: BridgeContextRequirement[] = ['ability'];

  async invokeAsync(
    action: string,
    request: BridgeTypedValue,
    _context: BridgeCallContext,
  ): Promise<BridgeTypedValue> {
    if (action !== 'export-text') {
      throw new Error(`Unsupported hopenconnect.export action '${action}'`);
    }
    if (request.typeName !== EXPORT_REQUEST_TYPE || typeof request.value !== 'object') {
      throw new Error(`hopenconnect.export expected ${EXPORT_REQUEST_TYPE}, got ${request.typeName}`);
    }
    const payload = request.value as ExportTextRequest;
    if (typeof payload.suggestedName !== 'string' || typeof payload.content !== 'string') {
      throw new Error(`hopenconnect.export invalid export request shape`);
    }
    await this.exportLog(payload);
    return { typeName: EXPORT_RESPONSE_TYPE, value: {} };
  }

  private async exportLog(payload: ExportTextRequest): Promise<void> {
    const content = payload.content ?? '';
    if (content.length === 0) {
      throw new Error('log export content is empty');
    }
    const baseName = (payload.suggestedName ?? 'h-openconnect-log')
      .replace(/[\\/:*?"<>|]/g, '_')
      .trim() || 'h-openconnect-log';
    const fileName = baseName.endsWith('.log') ? baseName : `${baseName}.log`;
    const documentPicker = new picker.DocumentViewPicker();
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
      hilog.info(LOG_DOMAIN, LOG_TAG, 'exported log');
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
}
