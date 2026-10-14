<script lang="ts">
  // Minimal SF Symbols-like line icons.
  interface Props { name: string; size?: number }
  let { name, size = 16 }: Props = $props();
  const paths: Record<string, string> = {
    play: "M7 4.5v15l12-7.5z",
    pause: "M7 4.5h3.5v15H7zM13.5 4.5H17v15h-3.5z",
    stop: "M6 6h12v12H6z",
    fade: "M3 6h4l13 12M3 6v12h17",
    next: "M5 5l9 7-9 7zM16 5h3v14h-3z",
    skip: "M4 12h12M12 7l5 5-5 5M20 5v14",
    back: "M18 5v14l-9-7zM6 5h2v14H6z",
    headphones: "M4 15v-3a8 8 0 0 1 16 0v3M4 15h3v5H5a1 1 0 0 1-1-1zM20 15h-3v5h2a1 1 0 0 0 1-1z",
    plus: "M12 5v14M5 12h14",
    minus: "M5 12h14",
    search: "M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13zM15.5 15.5L20 20",
    gear: "M12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7zM19.4 13.5l1.6 1.2-2 3.4-1.9-.7a7 7 0 0 1-2.1 1.2L14.7 21h-4l-.3-2a7 7 0 0 1-2.1-1.2l-1.9.7-2-3.4 1.6-1.2a7 7 0 0 1 0-2.6L4.4 10.1l2-3.4 1.9.7a7 7 0 0 1 2.1-1.2L10.7 3h4l.3 2.2a7 7 0 0 1 2.1 1.2l1.9-.7 2 3.4-1.6 1.2a7 7 0 0 1 0 2.6z",
    projector: "M3 7h18v10H3zM8 21l2-4M16 21l-2-4M15 12a2 2 0 1 0-4 0 2 2 0 0 0 4 0z",
    midi: "M4 6h16v12H4zM8 6v6M12 6v6M16 6v6",
    speaker: "M4 9h4l5-4v14l-5-4H4zM16.5 8.5a5 5 0 0 1 0 7M19 6a8.5 8.5 0 0 1 0 12",
    spotify: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM7.5 9.5c3-1 6.5-.7 9 .8M8 12.6c2.5-.7 5.2-.5 7.3.7M8.6 15.4c2-.5 3.9-.3 5.6.5",
    folder: "M3 6h6l2 2h10v10H3z",
    list: "M8 6h12M8 12h12M8 18h12M4 6h.01M4 12h.01M4 18h.01",
    crate: "M4 8h16v11H4zM3 5h18v3H3zM10 12h4",
    history: "M12 7v5l3 2M3.5 12a8.5 8.5 0 1 0 2.5-6M3 4v4h4",
    warning: "M12 4l9 16H3zM12 10v4M12 17h.01",
    check: "M5 12.5l4.5 4.5L19 7",
    x: "M6 6l12 12M18 6L6 18",
    auto: "M4 12a8 8 0 0 1 14-5.3M20 12a8 8 0 0 1-14 5.3M18 3v4h-4M6 21v-4h4",
    slot: "M5 5h14v14H5z M9 12h6",
    music: "M9 18V6l11-2v12M9 18a3 3 0 1 1-6 0 3 3 0 0 1 6 0zM20 16a3 3 0 1 1-6 0 3 3 0 0 1 6 0z",
    mic: "M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3zM5 11a7 7 0 0 0 14 0M12 18v3",
    tag: "M3 12V4h8l10 10-8 8zM7.5 7.5h.01",
    reload: "M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7",
    dots: "M5 12h.01M12 12h.01M19 12h.01",
    up: "M12 19V5M6 11l6-6 6 6",
    down: "M12 5v14M6 13l6 6 6-6",
    link: "M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1",
    info: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 11v6M12 7.5h.01",
    trash: "M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13",
    copy: "M8 8h12v12H8zM4 16V4h12",
  };
  const filled = new Set(["play", "pause", "stop", "next", "back"]);
</script>

<svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true"
  fill={filled.has(name) ? "currentColor" : "none"} stroke={filled.has(name) ? "none" : "currentColor"}
  stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
  <path d={paths[name] ?? paths.music} />
</svg>
