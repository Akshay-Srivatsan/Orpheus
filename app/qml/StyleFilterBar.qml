import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// Filters the library by your dance styles. Styles are labels on songs, so
// any view can be narrowed to one or more.
ColumnLayout {
    id: root

    /// The library view (`library_view`): its style counts and filter targets.
    property var view: null

    spacing: 4

    function toggle(key, add) {
        const sel = Session.styleFilter;
        if (add) Session.styleFilter = sel.includes(key) ? sel.filter(k => k !== key) : sel.concat([key]);
        else Session.styleFilter = sel.includes(key) && sel.length === 1 ? [] : [key];
        Session.selected = [];
    }

    /// Every style of yours (with how many songs here have it), then "No style".
    readonly property var items: !view ? [] : Session.myStyles.map(st => {
        const c = view.styleCounts.find(x => x.key === st.key);
        return { key: st.key, name: qsTr("%1 (%2)").arg(st.name).arg(c ? c.songs : 0), on: Session.styleFilter.includes(st.key) };
    }).concat([{ key: "__none", name: qsTr("No style (%1)").arg(view.noStyleSongs), on: Session.styleFilter.includes("__none") }])

    // Each narrows what's shown; together, songs must match all of them.
    Flow {
        Layout.fillWidth: true
        spacing: Theme.spacing
        RowLayout {
            Label { text: qsTr("Styles:") }
            CheckListBox {
                Layout.preferredWidth: 220
                items: root.items
                emptyText: qsTr("All styles")
                actionText: qsTr("All styles")
                actionEnabled: Session.styleFilter.length > 0
                onToggled: key => root.toggle(key, true)
                onAction: { Session.styleFilter = []; Session.selected = []; }
            }
        }
        RowLayout {
            Label { text: qsTr("Tags:") }
            CheckListBox {
                Layout.preferredWidth: 160
                enabled: Session.myTags.length > 0
                items: Session.myTags.map(t => ({ key: t.key, name: t.name, on: Session.filterTags.includes(t.key) }))
                emptyText: Session.myTags.length ? qsTr("Any") : qsTr("No tags yet")
                actionText: qsTr("Any tags")
                actionEnabled: Session.filterTags.length > 0
                onToggled: key => Session.filterTags = Session.filterTags.includes(key) ? Session.filterTags.filter(k => k !== key) : Session.filterTags.concat([key])
                onAction: Session.filterTags = []
            }
        }
        RowLayout {
            Label { text: qsTr("Meter:") }
            ComboBox {
                readonly property var values: ["", "duple", "triple", "isEmpty"]
                implicitContentWidthPolicy: ComboBox.WidestText
                model: [qsTr("Any"), qsTr("Duple (2/4, 4/4)"), qsTr("Triple (3/4, 6/8)"), qsTr("Unknown")]
                currentIndex: Math.max(0, values.indexOf(Session.filterMeter))
                onActivated: index => Session.filterMeter = values[index]
            }
        }
        RowLayout {
            Label { text: qsTr("Tempo:") }
            NumberField {
                implicitWidth: 64
                minimum: 0
                value: Session.filterBpmMin
                placeholderText: qsTr("from")
                onCommitted: v => Session.filterBpmMin = v
            }
            Label { text: "–" }
            NumberField {
                implicitWidth: 64
                minimum: 0
                value: Session.filterBpmMax
                placeholderText: qsTr("to")
                onCommitted: v => Session.filterBpmMax = v
            }
            Label { text: "BPM" }
        }
        Button {
            text: Session.filterMore.length ? qsTr("More Filters (%1)…").arg(Session.filterMore.length) : qsTr("More Filters…")
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Length, plays, last played, source, crate, text… (the rules smart crates use)")
            onClicked: Session.filtersEditRequested()
        }
        Button {
            text: qsTr("Save as Smart Crate…")
            enabled: Session.filtering
            ToolTip.visible: hovered
            ToolTip.text: qsTr("A smart crate of the songs these filters show (it follows the library)")
            onClicked: Session.saveFiltersRequested()
        }
        Button {
            text: qsTr("Clear Filters")
            enabled: Session.filtering
            onClicked: { Session.clearFilters(); Session.selected = []; }
        }
    }

    // A style filtered on, and what it's called at the club (with its tempos).
    RowLayout {
        visible: !!root.view && root.view.filterTargets.length > 0
        Layout.fillWidth: true
        spacing: Theme.spacing
        Label {
            text: Session.styleFilter.length === 1 ? qsTr("%1 is").arg(Session.styleName(Session.styleFilter[0])) : ""
            opacity: 0.7
        }
        Repeater {
            model: root.view ? root.view.filterTargets : []
            delegate: Label {
                required property var modelData
                textFormat: Text.StyledText
                text: "<font color='" + Theme.readable(modelData.color) + "'>" + modelData.name + "</font>"
                    + (modelData.sweet ? " " + modelData.sweet[0] + "–" + modelData.sweet[1] + " BPM" : "")
                    + (modelData.extended ? " <font color='" + Theme.system.placeholderText + "'>(" + qsTr("ok %1–%2").arg(modelData.extended[0]).arg(modelData.extended[1]) + ")</font>" : "")
            }
        }
        CheckBox {
            visible: !!root.view && root.view.filterTargets.some(t => t.sweet || t.extended)
            text: qsTr("include unlabelled songs that fit its tempo")
            checked: Session.suggestByTempo
            onToggled: Session.suggestByTempo = checked
        }
        Item { Layout.fillWidth: true }
    }
}
