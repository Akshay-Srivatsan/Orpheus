// Cover art by recording id (embedded in the file, an image next to it, or
// Spotify's), cached for the session.

import { api } from "./api";

const MAX = 64;
const cache = new Map<string, Promise<string | null>>();

export function artwork(recordingId: string): Promise<string | null> {
  let p = cache.get(recordingId);
  if (p) {
    // Most recently used last.
    cache.delete(recordingId);
  } else {
    p = api.artwork(recordingId).catch(() => null);
    if (cache.size >= MAX) cache.delete(cache.keys().next().value!);
  }
  cache.set(recordingId, p);
  return p;
}
