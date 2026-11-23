import * as tus from "tus-js-client";

import { apiJson } from "@/lib/api";

const ENDPOINT = "/api/uploads";
const CHUNK_SIZE = 50 * 1024 * 1024;
const MAX_PARALLEL = 3;
const RETRY_DELAYS = [0, 1000, 3000, 5000, 10000];

/** Same shape as `POST /api/bundles`. */
export type UploadBundleResponse = {
  id: string;
  files: string[];
  failed_files: string[];
};

function uploadFile(bundleId: string, file: File, onProgress: (sent: number) => void): Promise<void> {
  return new Promise((resolve, reject) => {
    const upload = new tus.Upload(file, {
      endpoint: ENDPOINT,
      chunkSize: CHUNK_SIZE,
      retryDelays: RETRY_DELAYS,
      removeFingerprintOnSuccess: true,
      metadata: {
        bundle_id: bundleId,
        filename: file.name,
        mtime: String(file.lastModified),
      },
      onError: (error) => reject(error),
      onProgress: (sent) => onProgress(sent),
      onSuccess: () => resolve(),
    });

    // After a reload the fingerprint remembers the partial upload: continue it.
    void upload.findPreviousUploads().then(
      (previous) => {
        if (previous.length > 0) upload.resumeFromPreviousUpload(previous[0]);
        upload.start();
      },
      (error: unknown) => reject(error),
    );
  });
}

/**
 * Creates an empty bundle and uploads every file into it with tus, resuming partial
 * uploads from earlier attempts. Three files at a time; `onProgress` is the total across
 * all of them. Files that still fail land in `failed_files`.
 */
export async function uploadToBundle(
  files: File[],
  onProgress: (sent: number, total: number) => void,
): Promise<UploadBundleResponse> {
  const bundle = await apiJson<{ id: string }>("POST", "/bundles/new");
  const total = files.reduce((sum, file) => sum + file.size, 0);
  const sent = new Array<number>(files.length).fill(0);
  const report = () => onProgress(sent.reduce((sum, bytes) => sum + bytes, 0), total);
  report();

  const uploaded: string[] = [];
  const failed: string[] = [];
  let next = 0;

  const worker = async () => {
    while (next < files.length) {
      const index = next++;
      const file = files[index];
      try {
        await uploadFile(bundle.id, file, (bytes) => {
          sent[index] = bytes;
          report();
        });
        uploaded.push(file.name);
      } catch {
        failed.push(file.name);
      }
      report();
    }
  };

  await Promise.all(Array.from({ length: Math.min(MAX_PARALLEL, files.length) }, worker));

  return { id: bundle.id, files: uploaded, failed_files: failed };
}
