pragma Singleton
import QtQuick
import Orpheus

// The interface's state: what the engine reports (kept current from its
// events) and what the DJ is looking at (views, selection, filters).
QtObject {
    id: session

    // ------------------------------------------------------------ engine data
    property var settings: null
    property var styles: null
    property var crates: []
    property var setlists: []
    property var clubs: []
    property var perform: null
    property var decks: [null, null]
    property var levels: null
    property var spotify: null
    property var scan: null
    /// What analysis is doing: { current: [{ id, title }], waiting }.
    property var analysis: null
    property var outputs: null
    property var stems: null
    property var tts: null
    property var midi: null
    /// The open set, as the set panel shows it (`set_view`).
    property var set: null
    /// The club in effect: the open set's, else the default club (`active_club`).
    property var activeClub: null
    /// Connected MIDI controllers.
    property var midiPorts: []

    readonly property var myStyles: styles ? styles.styles : []
    readonly property var myTags: styles && styles.tags ? styles.tags : []
    readonly property int liveDeck: perform ? perform.liveDeck : 0
    /// Where songs load: the engine's choice (deck A at the start).
    readonly property int freeDeck: perform ? perform.freeDeck : 0

    // ------------------------------------------------------------ what's shown
    property var view: ({ kind: "all" })
    property var styleFilter: []
    /// The library's other filters: tags (any of these), the meter (`duple`,
    /// `triple`, `isEmpty`; empty: any), a tempo range (BPM; null: open) and
    /// more rules (smart-crate conditions). All of them apply at once.
    property var filterTags: []
    property string filterMeter: ""
    property var filterBpmMin: null
    property var filterBpmMax: null
    property var filterMore: []
    readonly property var filterConditions: {
        const c = [];
        if (filterTags.length) c.push({ field: "tag", op: "hasAny", value: filterTags });
        if (filterMeter) c.push({ field: "meter", op: filterMeter });
        const lo = filterBpmMin, hi = filterBpmMax;
        if (lo !== null && hi !== null) c.push({ field: "bpm", op: "between", value: [Math.min(lo, hi), Math.max(lo, hi)] });
        else if (lo !== null) c.push({ field: "bpm", op: "atLeast", value: lo });
        else if (hi !== null) c.push({ field: "bpm", op: "atMost", value: hi });
        return c.concat(filterMore);
    }
    readonly property bool filtering: styleFilter.length > 0 || filterConditions.length > 0
    function clearFilters() {
        styleFilter = []; filterTags = []; filterMeter = ""; filterBpmMin = null; filterBpmMax = null; filterMore = [];
    }
    /// All the filters as one set of smart-crate rules (the styles too).
    function filterRules() {
        const styles = styleFilter.filter(k => k !== "__none");
        const c = styles.length ? [{ field: "style", op: "isAny", value: styles }]
            : styleFilter.includes("__none") ? [{ field: "style", op: "isEmpty" }] : [];
        return { match: "all", conditions: c.concat(filterConditions) };
    }
    /// The more-filters editor, or a new smart crate from the filters (Main opens them).
    signal filtersEditRequested()
    signal saveFiltersRequested()
    property string search: ""
    /// Sort column ("" = the view's own order) and direction.
    property string sort: ""
    property bool descending: false
    /// Songs whose other versions are unfolded in the library (track ids).
    property var unfolded: []
    property bool suggestByTempo: true
    /// The library lists versions you hid from it too.
    property bool showUnlisted: false
    /// Fits set entries' tempos to their styles (those playing outside them),
    /// and says how many changed.
    function fitTempos(entryIds) {
        return run("entries_fit_tempo", { ids: entryIds }).then(n => {
            if (n === undefined) return;
            toast(n ? qsTr("%n song(s) set to a tempo that fits its style", "", n) : qsTr("They already fit their styles' tempos"), "info");
        });
    }

    /// Hides versions from the library's lists, or shows them again.
    function setUnlisted(versionIds, value) {
        return Promise.all(versionIds.map(id => run("version_update", { id: id, patch: { unlisted: value } })));
    }
    /// Selected library rows (version ids) and set entries.
    property var selected: []
    property var selectedEntries: []
    /// The same, as lookups (rows ask whether they're selected: once each).
    /// The entry shown as up next (what Play Next or auto mode starts).
    readonly property var upNextRow: set && perform && perform.nextEntryId ? set.rows.find(r => r.entry.id === perform.nextEntryId) || null : null

    /// The set entry songs are added (or moved) after: the one the room hears
    /// (or heard last), else the last played; null: they go at the start.
    readonly property var insertAfterRow: set && perform && perform.insertAfter ? set.rows.find(r => r.entry.id === perform.insertAfter) || null : null
    readonly property string insertAfterName: !insertAfterRow ? "" : insertAfterRow.isPause ? qsTr("the pause") : insertAfterRow.title || insertAfterRow.label || qsTr("the slot")
    /// How the menus say where songs go (after that entry, else the start).
    readonly property string addAfterText: insertAfterRow ? qsTr("Add After “%1”").arg(insertAfterName) : qsTr("Add to the Start of the Set")
    readonly property string moveAfterText: insertAfterRow ? qsTr("Move After “%1”").arg(insertAfterName) : qsTr("Move to the Start of the Set")
    readonly property var selectedSet: { const m = {}; for (const id of selected) m[id] = true; return m; }
    readonly property var selectedEntriesSet: { const m = {}; for (const id of selectedEntries) m[id] = true; return m; }
    property string inspectTrack: ""
    property string inspectEntry: ""
    property string rightTab: "set"
    /// A style slot of the open set a song is being chosen for.
    property string fillingSlot: ""
    /// A song an edit's audio is being chosen for.
    property string pickingEdit: ""
    /// That song's title (for the banner).
    property string pickingEditTitle: ""
    /// Choosing a chime in the library: { title, done(versionId) }.
    property var chimePick: null
    function pickChime(title, done) { fillingSlot = ""; pickingEdit = ""; chimePick = { title: title, done: done }; }
    function chimePicked(versionId) {
        const p = chimePick;
        chimePick = null;
        if (p) p.done(versionId);
    }
    /// The open set's row of the slot being filled.
    readonly property var fillingRow: fillingSlot && set ? set.rows.find(r => r.entry.id === fillingSlot) || null : null
    // A slot filled or removed elsewhere (another window, a controller) stops being chosen.
    onSetChanged: if (fillingSlot && (!fillingRow || fillingRow.trackId)) fillingSlot = ""
    property string settingsPage: ""
    /// A crate whose name is being edited in the sidebar.
    property string renamingCrate: ""

    /// Asks the window for a confirmation (callback: confirmed) or a file
    /// (callback: its path). `options`: { title, nameFilters, save, name }.
    /// An action was refused because of where the crossfader is (it flashes).
    signal crossfaderBlocked()
    signal confirmRequested(string text, string okLabel, var callback)
    signal fileRequested(var options, var callback)

    function confirm(text, okLabel, callback) { confirmRequested(text, okLabel || qsTr("Delete"), callback); }
    function chooseFile(options, callback) { fileRequested(options, callback); }

    /// The default choice of what to say, given what's chosen above (`text`:
    /// the words, "" = nothing, null = your settings).
    function announcementDefault(text) {
        if (text === null || text === undefined)
            return settings && settings.announce.enabled ? qsTr("Default (a random sentence)") : qsTr("Default (nothing: announcements off)");
        return text ? qsTr("Default (“%1”)").arg(text) : qsTr("Default (nothing)");
    }

    /// Counts for the sidebar (`library_summary`).
    property var summary: null
    function refreshSummary() { Backend.call("library_summary").then(v => summary = v); }

    signal libraryChanged(var ids)
    signal setlistChanged(string id)
    signal historyChanged()
    signal recordingsChanged()

    // ------------------------------------------------------------ toasts
    property ListModel toasts: ListModel {}
    property var toastActions: ({})
    property int toastSeq: 0

    /// Shows a message; `action` = { label, run } adds a button.
    function toast(text, kind, action) {
        kind = kind || "error";
        for (let i = 0; i < toasts.count; i++) {
            const t = toasts.get(i);
            if (t.text === text && t.kind === kind) return;
        }
        const id = ++toastSeq;
        if (action) toastActions[id] = action.run;
        toasts.append({ toastId: id, text: text, kind: kind, actionLabel: action ? action.label : "" });
        const ms = action ? Theme.toastActionMs : kind === "error" ? Theme.toastErrorMs : Theme.toastMs;
        const timer = Qt.createQmlObject("import QtQuick; Timer { repeat: false }", session);
        timer.interval = ms;
        timer.triggered.connect(() => { dismiss(id); timer.destroy(); });
        timer.start();
    }

    function dismiss(id) {
        for (let i = 0; i < toasts.count; i++) {
            if (toasts.get(i).toastId === id) { toasts.remove(i); break; }
        }
        delete toastActions[id];
    }

    function runToastAction(id) {
        const f = toastActions[id];
        dismiss(id);
        if (f) f();
    }

    // ------------------------------------------------------------ calls

    /// Runs an engine command; errors become toasts (the result is then undefined).
    function run(name, args) {
        return Backend.call(name, args).catch(e => { toast(String(e)); return undefined; });
    }

    /// What can be undone, latest last: { id, text }.
    property var undos: []

    /// Runs an action (one or more calls), then offers to undo all it changed.
    function undoable(text, action) {
        return Backend.call("undo_begin").then(id => {
            const finish = r => Backend.call("undo_end", { id: id }).then(changed => {
                if (changed) {
                    undos = undos.concat([{ id: id, text: text }]);
                    toast(text, "info", { label: qsTr("Undo"), run: () => undo(id) });
                }
                return r;
            });
            return Promise.resolve(action()).then(finish, e => finish(undefined).then(() => { throw e; }));
        }).catch(e => { toast(String(e)); return undefined; });
    }

    /// Undoes one action (the latest: `id` undefined).
    function undo(id) {
        const u = id === undefined ? undos[undos.length - 1] : undos.find(x => x.id === id);
        if (!u) return;
        undos = undos.filter(x => x.id !== u.id);
        run("undo", { id: u.id });
    }

    /// Changes some settings (the rest stay as they are).
    function saveSettings(patch) {
        if (!settings) return;
        const next = Object.assign({}, settings, patch);
        settings = next;
        run("settings_set", { settings: next });
    }

    /// Something is being dragged (songs, set entries). Views keep their rows
    /// meanwhile: replacing them would destroy the row being dragged, or the
    /// one it's over, while the drag still uses it (that crashed). They catch
    /// up once it's over (`draggingChanged`).
    property bool dragging: false
    property bool setMissed: false
    onDraggingChanged: if (!dragging && setMissed) refreshSet()

    function refreshSet() {
        setMissed = false;
        const id = perform ? perform.setlistId : null;
        if (dragging) { setMissed = true; return; }
        if (!id) { set = null; return; }
        Backend.call("set_view", { id: id }).then(v => { if (dragging) setMissed = true; else set = v; }, () => set = null);
    }
    function refreshClub() { Backend.call("active_club").then(v => activeClub = v); }
    function refreshMidi() { Backend.call("midi_info").then(m => midiPorts = m.ports, () => {}); }
    readonly property Timer midiPoll: Timer { interval: 4000; running: true; repeat: true; onTriggered: session.refreshMidi() }

    function refreshSetlists() { Backend.call("setlists_list").then(v => setlists = v); }
    function refreshCrates() { Backend.call("crates_list").then(v => crates = v); }
    function refreshStyles() { Backend.call("styles_get").then(v => styles = v); }
    function refreshClubs() { Backend.call("clubs_list").then(v => clubs = v); }

    function start() {
        Backend.call("settings_get").then(v => settings = v);
        Backend.call("perform_state").then(v => { perform = v; refreshSet(); });
        Backend.call("deck_states").then(v => decks = v);
        Backend.call("spotify_status").then(v => spotify = v);
        Backend.call("scan_progress").then(v => scan = v);
        Backend.call("audio_status").then(v => outputs = v);
        Backend.call("stems_status").then(v => stems = v);
        Backend.call("tts_status").then(v => tts = v);
        Backend.call("library_info").then(v => {
            if (v.loadErrors.length) toast(qsTr("%1 library file(s) could not be read — see Settings → Library").arg(v.loadErrors.length));
        });
        refreshSetlists();
        refreshCrates();
        refreshStyles();
        refreshClubs();
        refreshClub();
        refreshMidi();
        refreshSummary();
    }

    // The sidebar's counts follow the library, crates and the club.
    onLibraryChanged: summaryTimer.restart()
    onActiveClubChanged: summaryTimer.restart()
    onCratesChanged: summaryTimer.restart()
    readonly property Timer summaryTimer: Timer { interval: 150; onTriggered: session.refreshSummary() }

    readonly property Connections events: Connections {
        target: Backend
        function onEvent(name, p) {
            switch (name) {
            case "deck-state": session.decks = p; break;
            case "levels": session.levels = p; break;
            case "perform-state": session.perform = p; session.refreshSet(); session.refreshClub(); break;
            case "library-changed": session.libraryChanged(p); session.refreshSet(); session.refreshStyles(); break;
            case "recordings-changed": session.recordingsChanged(); break;
            case "setlist-changed":
                session.setlistChanged(p);
                if (session.perform && p === session.perform.setlistId) session.refreshSet();
                session.refreshSetlists();
                break;
            case "setlists-changed": session.refreshSetlists(); break;
            case "crates-changed": session.refreshCrates(); session.libraryChanged([]); break;
            case "clubs-changed": session.refreshClubs(); session.refreshSetlists(); session.refreshSet(); session.refreshClub(); break;
            case "styles-changed": session.refreshStyles(); session.libraryChanged([]); break;
            case "settings-changed": session.settings = p; session.refreshClub(); break;
            case "scan-progress": session.scan = p; break;
            case "analysis": session.analysis = p; break;
            case "spotify-status": session.spotify = p; break;
            case "output-status": session.outputs = p; break;
            case "stems-status": session.stems = p; break;
            case "tts-status": session.tts = p; break;
            case "history-changed": session.historyChanged(); break;
            case "toast": session.toast(String(p)); break;
            case "toast-info": session.toast(String(p), "info"); break;
            case "open-settings": session.settingsPage = String(p); break;
            case "crossfader-blocked":
                session.crossfaderBlocked();
                session.toast(qsTr("The crossfader is toward deck %1: center it, or move it to the deck you want on air.").arg(p === 0 ? "A" : "B"), "info");
                break;
            case "reconnect-found":
                for (const m of p) {
                    session.toast(qsTr("A new file sounds like missing audio a song still plays."), "info",
                        { label: qsTr("Use It"), run: () => session.run("reconnect", { missing: m.missing, found: m.found }) });
                }
                break;
            }
        }
    }

    // ------------------------------------------------------------ actions

    /// Adds songs to the open set: "next", "end" or a position. While a style
    /// is filtered on, they're danced as it; with a slot selected, one song fills it.
    /// What can be dropped as songs: library versions, or Spotify tracks.
    readonly property var songKeys: ["application/x-orpheus-versions", "application/x-orpheus-spotify"]
    /// The versions a drop carries (a promise): library songs as they are,
    /// Spotify tracks made into versions (`library`: they join the library too).
    /// The drop's data is read right away (it's gone once the handler returns).
    /// A drop's data in `format`: from a drag in the window (`DragLabel`), else the platform's.
    function dropText(drop, format) {
        return drop.source && drop.source.payload ? drop.source.payload[format] || "" : drop.getDataAsString(format);
    }
    /// The drop carries `format` (in-window drags say so in their keys).
    function dropHas(drop, format) {
        return drop.source && drop.source.payload ? format in drop.source.payload : drop.formats.includes(format);
    }
    function droppedVersions(drop, library) {
        if (dropHas(drop, "application/x-orpheus-versions"))
            return Promise.resolve(JSON.parse(dropText(drop, "application/x-orpheus-versions") || "[]"));
        if (dropHas(drop, "application/x-orpheus-spotify")) {
            const tracks = JSON.parse(dropText(drop, "application/x-orpheus-spotify") || "[]");
            return run("spotify_versions", { tracks: tracks, inLibrary: !!library }).then(v => v || []);
        }
        return Promise.resolve([]);
    }

    function addToSet(versionIds, where) {
        const position = typeof where === "number" ? { at: where } : where;
        const style = styleFilter.length === 1 && styleFilter[0] !== "__none" ? styleFilter[0] : null;
        const slot = selectedEntries.length === 1 ? selectedEntries[0] : null;
        return run("set_add", { versionIds: versionIds, position: position, style: style, slot: slot });
    }

    /// Loads a song onto a deck (paused).
    /// Loads a song (paused) onto `deck`, or (null) the free deck as the engine decides.
    function loadDeck(deck, versionId, entryId) {
        return run("deck_load", { deck: deck, versionId: versionId, entryId: entryId || null, play: false });
    }

    /// Plays a song in the headphones on the free deck.
    function preview(versionId, entryId) {
        return run("deck_load", { deck: null, versionId: versionId, entryId: entryId || null, play: true });
    }

    function fillSlot(versionId, slotId) {
        const id = slotId || fillingSlot;
        if (!id) return;
        run("entry_update", { id: id, patch: { versionId: versionId } }).then(r => { if (r !== undefined && fillingSlot === id) fillingSlot = ""; });
    }

    function startFillingSlot(entry) {
        fillingSlot = entry.id;
        if (entry.styles && entry.styles.length) styleFilter = entry.styles.slice();
        selected = [];
    }

    /// A new crate with these songs, named in the sidebar.
    function newCrateWith(versionIds) {
        run("crate_create", { name: qsTr("New crate"), smart: null }).then(id => {
            if (!id) return;
            if (versionIds && versionIds.length) run("crate_add", { id: id, versionIds: versionIds });
            renamingCrate = id;
        });
    }

    /// Adds a version playing other audio (an edit) to a song: `audio` is
    /// { path }, { spotify } or { recording }. It can be undone.
    function addEdit(audio, trackId) {
        const id = trackId || pickingEdit;
        if (!id) return;
        undoable(qsTr("Added a version"), () => run("version_add_audio", { trackId: id, audio: audio })).then(v => {
            if (!v) return;
            if (pickingEdit === id) pickingEdit = "";
            inspectTrack = id;
            rightTab = "info";
        });
    }

    /// Puts text on the clipboard (`what` names it in the confirmation).
    function copy(text, what) {
        if (!text) return;
        clipboard.text = text;
        clipboard.selectAll();
        clipboard.copy();
        toast(qsTr("%1 copied.").arg(what), "info");
    }
    readonly property TextEdit clipboard: TextEdit { visible: false }

    function show(v) {
        view = v;
        selected = [];
    }

    // ------------------------------------------------------------ formatting

    function fmtTime(ms, tenths) {
        if (ms === null || ms === undefined || !isFinite(ms)) return "–:––";
        const neg = ms < 0;
        const total = Math.abs(ms) / 1000;
        const m = Math.floor(total / 60);
        const s = Math.floor(total % 60);
        let out = m + ":" + (s < 10 ? "0" : "") + s;
        if (tenths) out += "." + Math.floor((total * 10) % 10);
        return (neg ? "−" : "") + out;
    }

    function fmtBpm(b) {
        if (b === null || b === undefined) return "–";
        return Math.abs(b - Math.round(b)) < 0.05 ? String(Math.round(b)) : b.toFixed(1);
    }

    /// "+3.5%" / "−3.5%" for a playback rate.
    function fmtPct(rate) {
        const p = (rate - 1) * 100;
        if (Math.abs(p) < 0.05) return "0.0%";
        return (p > 0 ? "+" : "−") + Math.abs(p).toFixed(1) + "%";
    }

    function fmtDb(db) {
        return (db > 0 ? "+" : db < 0 ? "−" : "") + Math.abs(db).toFixed(1) + " dB";
    }

    function meterName(n) {
        return n === 3 || n === 6 ? qsTr("triple") : n === 2 || n === 4 ? qsTr("duple") : "";
    }

    function styleName(key) {
        const s = myStyles.find(x => x.key === key);
        return s ? s.name : key;
    }
}
