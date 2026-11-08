// Read a .zip in the browser, so a skin can be uploaded as one file without
// the server needing a zip library. Handles stored and deflated entries (what
// every zip tool writes); anything else is reported as unsupported.

import { i18n } from "@/i18n";

export interface ZipEntry {
  path: string;
  data: Uint8Array;
}

const u16 = (b: DataView, o: number) => b.getUint16(o, true);
const u32 = (b: DataView, o: number) => b.getUint32(o, true);

async function inflate(data: Uint8Array): Promise<Uint8Array> {
  const stream = new Blob([data as BlobPart]).stream().pipeThrough(new DecompressionStream("deflate-raw"));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

/** Every file in the archive (folders and macOS metadata left out). */
export async function readZip(file: Blob, limits = { files: 96, bytes: 8 * 1024 * 1024 }): Promise<ZipEntry[]> {
  const buf = new Uint8Array(await file.arrayBuffer());
  const view = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
  // The end-of-central-directory record is in the last 64 KB + 22 bytes.
  let eocd = -1;
  for (let i = buf.length - 22; i >= Math.max(0, buf.length - 65_557); i--) {
    if (u32(view, i) === 0x06054b50) {
      eocd = i;
      break;
    }
  }
  if (eocd < 0) throw new Error(i18n.t("skins.zip.not_zip"));
  const count = u16(view, eocd + 10);
  let p = u32(view, eocd + 16);
  const out: ZipEntry[] = [];
  let total = 0;
  const names = new TextDecoder();
  for (let n = 0; n < count; n++) {
    if (u32(view, p) !== 0x02014b50) throw new Error(i18n.t("skins.zip.damaged"));
    const method = u16(view, p + 10);
    const compressed = u32(view, p + 20);
    const size = u32(view, p + 24);
    const nameLen = u16(view, p + 28);
    const extraLen = u16(view, p + 30);
    const commentLen = u16(view, p + 32);
    const local = u32(view, p + 42);
    const path = names.decode(buf.subarray(p + 46, p + 46 + nameLen));
    p += 46 + nameLen + extraLen + commentLen;
    if (path.endsWith("/") || path.startsWith("__MACOSX/") || path.split("/").pop()?.startsWith("._")) continue;
    if (path.split("/").pop() === ".DS_Store") continue;
    if (out.length >= limits.files) throw new Error(i18n.t("skins.zip.too_many_files", { count: limits.files }));
    total += size;
    if (total > limits.bytes) throw new Error(i18n.t("skins.zip.too_big"));
    const start = local + 30 + u16(view, local + 26) + u16(view, local + 28);
    const raw = buf.subarray(start, start + compressed);
    let data: Uint8Array;
    if (method === 0) data = raw;
    else if (method === 8) data = await inflate(raw);
    else throw new Error(i18n.t("skins.zip.unsupported_compression", { path }));
    out.push({ path, data });
  }
  return out;
}
