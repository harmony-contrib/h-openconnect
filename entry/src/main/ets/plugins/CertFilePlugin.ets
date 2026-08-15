import { AsyncPluginBase, BridgeCallContext, BridgeContextRequirement, BridgeTypedValue } from '@ohos-rs/ability';
import { fileIo, picker } from '@kit.CoreFileKit';
import { hilog } from '@kit.PerformanceAnalysisKit';
import hopenconnectUi from 'libhopenconnect_ui.so';
import { describeError } from '../common/Errors';

const LOG_DOMAIN: number = 0x0000;
const LOG_TAG: string = 'HOpenConnectCertFile';

const CERT_FILE_REQUEST_TYPE = 'hopenconnect.CertFileRequest';
const CERT_FILE_RESPONSE_TYPE = 'hopenconnect.CertFileResponse';

interface CertFileRequest {
  kind: string;
}

/**
 * `hopenconnect.cert-file` — selects a client/CA certificate with the system
 * document picker, copies it into the app sandbox and returns the real
 * filesystem path OpenConnect can open.
 */
export class CertFilePlugin extends AsyncPluginBase {
  readonly id = 'hopenconnect.cert-file';
  readonly requires: BridgeContextRequirement[] = ['ability'];

  async invokeAsync(
    action: string,
    request: BridgeTypedValue,
    context: BridgeCallContext,
  ): Promise<BridgeTypedValue> {
    if (action !== 'pick-cert') {
      throw new Error(`Unsupported hopenconnect.cert-file action '${action}'`);
    }
    if (request.typeName !== CERT_FILE_REQUEST_TYPE || typeof request.value !== 'object') {
      throw new Error(`hopenconnect.cert-file expected ${CERT_FILE_REQUEST_TYPE}, got ${request.typeName}`);
    }
    const payload = request.value as CertFileRequest;
    const kind = typeof payload.kind === 'string' ? payload.kind : 'certificate';
    const path = await this.pickAndStoreCertFile(kind, context);
    return { typeName: CERT_FILE_RESPONSE_TYPE, value: { path } };
  }

  private certDir(filesDir: string): string {
    return `${filesDir}/h-openconnect/certs`;
  }

  private ensureCertDir(dir: string): void {
    try {
      if (!fileIo.accessSync(dir)) {
        fileIo.mkdirSync(dir, true);
      }
    } catch (_err) {
      try {
        fileIo.mkdirSync(dir, true);
      } catch (mkdirErr) {
        hilog.error(LOG_DOMAIN, LOG_TAG, 'mkdir certs failed %{public}s', describeError(mkdirErr));
      }
    }
  }

  private async pickAndStoreCertFile(kind: string, context: BridgeCallContext): Promise<string> {
    const dir = this.certDir(context.abilityContext.filesDir);
    this.ensureCertDir(dir);
    const options = new picker.DocumentSelectOptions();
    options.maxSelectNumber = 1;
    // PEM / DER / PKCS#12 common suffixes (generic, not vendor-specific).
    options.fileSuffixFilters = ['.pem', '.crt', '.cer', '.key', '.p12', '.pfx', '.der', '.ca'];
    const documentPicker = new picker.DocumentViewPicker(context.abilityContext);
    const uris: Array<string> = await documentPicker.select(options);
    if (!uris || uris.length === 0) {
      return '';
    }
    const srcUri = uris[0];
    const baseName = this.fileNameFromUri(srcUri, kind);
    const destPath = `${dir}/${Date.now()}_${baseName}`;
    await this.copyUriToPath(srcUri, destPath);
    return destPath;
  }

  private fileNameFromUri(uri: string, kind: string): string {
    try {
      const noQuery = uri.split('?')[0];
      const segments = noQuery.split('/');
      let last = segments.length > 0 ? segments[segments.length - 1] : '';
      last = decodeURIComponent(last);
      if (last && last.indexOf('.') > 0 && last.length < 128) {
        // Sanitize path separators that should not appear in a leaf name.
        return last.replace(/[\\/]/g, '_');
      }
    } catch (_err) {
      // fall through
    }
    const ext = kind === 'private_key' ? 'key' : kind === 'ca_certificate' ? 'crt' : 'pem';
    return `${kind}.${ext}`;
  }

  private async copyUriToPath(srcUri: string, destPath: string): Promise<void> {
    let srcFile: fileIo.File | undefined = undefined;
    let destFile: fileIo.File | undefined = undefined;
    let completed = false;
    try {
      srcFile = fileIo.openSync(srcUri, fileIo.OpenMode.READ_ONLY);
      destFile = fileIo.openSync(
        destPath,
        fileIo.OpenMode.READ_WRITE | fileIo.OpenMode.CREATE | fileIo.OpenMode.TRUNC,
      );
      // Stream copy — certs are small; cap total size to avoid abuse.
      const chunk = new ArrayBuffer(64 * 1024);
      let total = 0;
      const maxBytes = 8 * 1024 * 1024;
      while (true) {
        const n: number = fileIo.readSync(srcFile.fd, chunk);
        if (n <= 0) {
          break;
        }
        total += n;
        if (total > maxBytes) {
          throw new Error(`certificate file too large (>${maxBytes} bytes)`);
        }
        // writeSync accepts ArrayBuffer; when partial read, write a sliced view.
        if (n === chunk.byteLength) {
          fileIo.writeSync(destFile.fd, chunk);
        } else {
          fileIo.writeSync(destFile.fd, chunk.slice(0, n));
        }
      }
      if (total === 0) {
        throw new Error('selected certificate file is empty');
      }
      hopenconnectUi.securePrivateFile(destPath);
      completed = true;
    } finally {
      if (srcFile) {
        try {
          fileIo.closeSync(srcFile);
        } catch (_e) {
          // ignore
        }
      }
      if (destFile) {
        try {
          fileIo.closeSync(destFile);
        } catch (_e) {
          // ignore
        }
      }
      if (!completed) {
        try {
          fileIo.unlinkSync(destPath);
        } catch (_e) {
          // The destination may not have been created.
        }
      }
    }
  }
}
