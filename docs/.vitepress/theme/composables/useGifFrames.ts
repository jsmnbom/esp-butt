/**
 * Decode all frames of an animated GIF into an array of ImageBitmaps suitable
 * for random-access frame lookup in the render loop.
 *
 * Uses the WebCodecs ImageDecoder when available (Chrome, Firefox), and falls
 * back to the pure-JS gifuct-js decoder for Safari and older browsers.
 */
export async function useGifFrames(url: string): Promise<ImageBitmap[]> {
  const res = await fetch(url);
  const buf = await res.arrayBuffer();

  if (typeof ImageDecoder !== "undefined") {
    // Fast path: WebCodecs (Chrome, Firefox)
    const decoder = new ImageDecoder({ data: buf, type: "image/gif" });
    await decoder.tracks.ready;
    const frameCount = decoder.tracks.selectedTrack!.frameCount;
    const bitmaps: ImageBitmap[] = [];
    for (let i = 0; i < frameCount; i++) {
      const { image } = await decoder.decode({ frameIndex: i });
      bitmaps.push(await createImageBitmap(image));
      image.close();
    }
    decoder.close();
    return bitmaps;
  }

  // Fallback: gifuct-js pure-JS decoder (Safari)
  const { parseGIF, decompressFrames } = await import("gifuct-js");
  const gif = parseGIF(buf);
  const frames = decompressFrames(gif, true);
  const { width, height } = gif.lsd;
  const offscreen = document.createElement("canvas");
  offscreen.width = width;
  offscreen.height = height;
  const ctx = offscreen.getContext("2d")!;
  const bitmaps: ImageBitmap[] = [];
  for (const frame of frames) {
    ctx.putImageData(
      new ImageData(new Uint8ClampedArray(frame.patch), frame.dims.width, frame.dims.height),
      frame.dims.left,
      frame.dims.top,
    );
    bitmaps.push(await createImageBitmap(offscreen));
  }
  return bitmaps;
}
