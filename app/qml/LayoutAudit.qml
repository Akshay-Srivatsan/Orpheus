import QtQuick
import QtQuick.Controls

// A development aid (run with --audit-layout): lists visible items that stick
// out of their parent, items in a row that overlap the next, and controls
// too narrow for their text, to find controls spilling out of (or squeezed
// in) their containers at a given window size.
QtObject {
    id: audit
    /// Pixels of slack before something counts.
    readonly property real slack: 1.5

    function name(item) {
        let n = String(item).replace(/_QMLTYPE_\d+|\(0x[0-9a-f]+\)|QQuick/g, "").replace(/\s.*$/, "");
        const text = typeof item.text === "string" && item.text ? item.text : typeof item.title === "string" ? item.title : "";
        return text ? n + " “" + text.slice(0, 30) + "”" : n;
    }
    function path(item, depth) {
        const parts = [];
        for (let p = item; p && parts.length < depth; p = p.parent) parts.unshift(name(p));
        return parts.join(" › ");
    }
    // Contents meant to scroll or move: a Flickable's content, popups, the
    // style's decorations (backgrounds, indicators).
    function scrolls(item) {
        return item.parent && item.parent.contentItem === item && item.parent.flicking !== undefined;
    }
    function check(item, out) {
        if (!item || !item.visible || item.opacity === 0 || item.width <= 0) return;
        const kids = item.children;
        const row = /RowLayout|Row\b/.test(String(item));
        let prev = null;
        for (let i = 0; i < kids.length; i++) {
            const k = kids[i];
            if (!k.visible || k.opacity === 0 || (k.width <= 0 && k.height <= 0)) continue;
            // Scrolling content may be taller than its view, not wider (nothing here scrolls sideways).
            if (scrolls(item) && item.parent.width > 0 && k.x + k.width > item.parent.width + slack)
                out.push("WIDER THAN ITS VIEW " + Math.round(k.x + k.width - item.parent.width) + "px: " + path(k, 5));
            if (!scrolls(item) && !item.clip && item.width > 0 && item.height > 0) {
                const over = Math.max(-k.x, -k.y, k.x + k.width - item.width, k.y + k.height - item.height);
                if (over > slack && !(k === item.background) && !/Ripple|Rectangle|DropArea|MouseArea|Canvas/.test(String(k)))
                    out.push("OVERFLOW " + Math.round(over) + "px: " + path(k, 5) + " (" + Math.round(k.width) + "×" + Math.round(k.height) + " in " + Math.round(item.width) + "×" + Math.round(item.height) + ")");
            }
            // A button (or field) narrower than its content: its text is cut short.
            if (k.availableWidth !== undefined && k.implicitContentWidth !== undefined && k.text && k.availableWidth + slack < k.implicitContentWidth)
                out.push("SQUEEZED " + Math.round(k.implicitContentWidth - k.availableWidth) + "px: " + path(k, 4));
            if (row && prev && k.x < prev.x + prev.width - slack && Math.abs(k.y - prev.y) < k.height)
                out.push("OVERLAP " + Math.round(prev.x + prev.width - k.x) + "px: " + path(prev, 4) + " / " + name(k));
            prev = k;
            check(k, out);
        }
    }
    function run(root, label) {
        const out = [];
        check(root, out);
        console.warn("AUDIT " + label + ": " + out.length + " problem(s)");
        for (const line of out) console.warn("AUDIT   " + line);
    }
}
