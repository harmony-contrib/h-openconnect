import { common } from "@kit.AbilityKit";
import { fileIo, picker } from "@kit.CoreFileKit";
import { hilog } from "@kit.PerformanceAnalysisKit";
import hopenconnectUi from "libhopenconnect_ui.so";
import { describeError } from "../common/Errors";

const LOG_DOMAIN: number = 0x0000;
const LOG_TAG: string = "HOpenConnectCertFile";

/**
 * Selects a client/CA certificate or private key via the system document
 * picker, copies it into app-private storage with restricted permissions,
 * and returns the filesystem path OpenConnect can open.
 */
export class CertFileService {
  private readonly context: common.UIAbilityContext;

  constructor(context: common.UIAbilityContext) {
    this.context = context;
  }

  async pickCertFile(kind: string): Promise<string> {
    const dir = `${this.context.filesDir}/h-openconnect/certs`;
    this.ensureCertDir(dir);
    const options = new picker.DocumentSelectOptions();
    options.maxSelectNumber = 1;
    options.fileSuffixFilters = [".pem", ".crt", ".cer", ".key", ".p12", ".pfx", ".der", ".ca"];
    const documentPicker = new picker.DocumentViewPicker(this.context);
    const uris: Array<string> = await documentPicker.select(options);
    if (!uris || uris.length === 0) {
      return "";
    }
    const srcUri = uris[0];
    const baseName = this.fileNameFromUri(srcUri, kind);
    const destPath = `${dir}/${Date.now()}_${baseName}`;
    await this.copyUriToPath(srcUri, destPath);
    return destPath;
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
        hilog.error(LOG_DOMAIN, LOG_TAG, "mkdir certs failed %{public}s", describeError(mkdirErr));
      }
    }
  }

  private fileNameFromUri(uri: string, kind: string): string {
    try {
      const noQuery = uri.split("?")[0];
      const segments = noQuery.split("/");
      let last = segments.length > 0 ? segments[segments.length - 1] : "";
      last = decodeURIComponent(last);
      if (last && last.indexOf(".") > 0 && last.length < 128) {
        return last.replace(/[\\/]/g, "_");
      }
    } catch (_err) {
      // fall through
    }
    const ext = kind === "private_key" ? "key" : kind === "ca_certificate" ? "crt" : "pem";
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
        if (n === chunk.byteLength) {
          fileIo.writeSync(destFile.fd, chunk);
        } else {
          fileIo.writeSync(destFile.fd, chunk.slice(0, n));
        }
      }
      if (total === 0) {
        throw new Error("selected certificate file is empty");
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
          // destination may not have been created
        }
      }
    }
  }
}
