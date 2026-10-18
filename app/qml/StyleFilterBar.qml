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

    RowLayout {
        Label { text: qsTr("Styles:") }
        CheckListBox {
            Layout.preferredWidth: 260
            items: root.items
            emptyText: qsTr("All styles")
            actionText: qsTr("All styles")
            actionEnabled: Session.styleFilter.length > 0
            onToggled: key => root.toggle(key, true)
            onAction: { Session.styleFilter = []; Session.selected = []; }
            ToolTip.text: qsTr("Show only songs of these styles (Dance Styles in a song's menu labels them)")
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
