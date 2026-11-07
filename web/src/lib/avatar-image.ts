// Shrinks a picked photo to a small square before upload, so a 12 MB phone
// photo becomes a ~20 KB profile picture and the server never stores big files.

export const AVATAR_SIZE = 256;

/** Where to cut the largest centred square out of a w×h image. */
export function centerSquare(w: number, h: number) {
  const side = Math.min(w, h);
  return { sx: Math.round((w - side) / 2), sy: Math.round((h - side) / 2), side };
}

export async function shrinkToAvatar(file: File, size = AVATAR_SIZE): Promise<Blob> {
  if (!file.type.startsWith("image/")) throw new Error("Pick an image file.");
  const bitmap = await createImageBitmap(file).catch(() => {
    throw new Error("That image couldn't be read. Try a PNG or JPEG.");
  });
  const { sx, sy, side } = centerSquare(bitmap.width, bitmap.height);
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = size;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("Your browser can't resize images.");
  ctx.imageSmoothingQuality = "high";
  ctx.drawImage(bitmap, sx, sy, side, side, 0, 0, size, size);
  bitmap.close();
  const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/webp", 0.85));
  // Safari can't encode WebP and hands back PNG (or nothing); JPEG always works.
  if (blob && (blob.type === "image/webp" || blob.type === "image/png")) return blob;
  const jpeg = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/jpeg", 0.88));
  if (!jpeg) throw new Error("Couldn't prepare the picture.");
  return jpeg;
}
