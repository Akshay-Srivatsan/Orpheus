// Cover art by track id (embedded in the file, an image next to it, or
// Spotify's), cached for the session.

import { api } from "./api";

const MAX = 64;
const cache = new Map<string, Promise<string | null>>();

export function artwork(trackId: string): Promise<string | null> {
  let p = cache.get(trackId);
  if (p) {
    // Most recently used last.
    cache.delete(trackId);
  } else {
    p = api.artwork(trackId).catch(() => null);
    if (cache.size >= MAX) cache.delete(cache.keys().next().value!);
  }
  cache.set(trackId, p);
  return p;
}
