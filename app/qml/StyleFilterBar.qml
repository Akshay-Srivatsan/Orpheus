import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// Filters the library by your dance styles. Styles are labels on songs, so
// any view can be narrowed to one or more; dropping songs on a style labels them.
ColumnLayout {
    id: root

    /// The library view (`library_view`): its chips and filter targets.
    property var view: null

    spacing: 4

    function toggle(key, add) {
        const sel = Session.styleFilter;
        if (add) Session.styleFilter = sel.includes(key) ? sel.filter(k => k !== key) : sel.concat([key]);
        else Session.styleFilter = sel.includes(key) && sel.length === 1 ? [] : [key];
        Session.selected = [];
    }

    function label(key, ids) {
        if (!ids.length) return;
        if (key === "__none") Session.run("style_remove", { versionIds: ids, style: null });
        else Session.run("style_add", { style: key, versionIds: ids });
    }

    Flow {
        Layout.fillWidth: true
        spacing: 4

        StyleChip {
            text: qsTr("All styles")
            checked: !Session.styleFilter.length
            onChosen: Session.styleFilter = []
        }
        Repeater {
            model: root.view ? root.view.styleCounts : []
            delegate: StyleChip {
                required property var modelData
                text: modelData.short
                count: modelData.songs
                color: modelData.color
                checked: Session.styleFilter.includes(modelData.key)
                tip: qsTr("%1 — click to filter, Ctrl/Shift-click to add; drop songs here to label them").arg(modelData.name)
                onChosen: add => root.toggle(modelData.key, add)
                onDropped: ids => root.label(modelData.key, ids)
            }
        }
        StyleChip {
            visible: !!root.view && (root.view.noStyleSongs > 0 || Session.styleFilter.includes("__none"))
            text: qsTr("No style")
            count: root.view ? root.view.noStyleSongs : 0
            checked: Session.styleFilter.includes("__none")
            tip: qsTr("Songs without a dance style — drop songs here to remove their styles")
            onChosen: add => root.toggle("__none", add)
            onDropped: ids => root.label("__none", ids)
        }
        ComboBox {
            implicitContentWidthPolicy: ComboBox.WidestText
            readonly property var hidden: root.view ? Session.myStyles.filter(s => !root.view.styleCounts.some(c => c.key === s.key)) : []
            visible: hidden.length > 0
            model: [qsTr("More…")].concat(hidden.map(s => s.name))
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Filter by a style with no songs here yet (e.g. to label some)")
            onActivated: index => {
                if (index > 0) Session.styleFilter = [hidden[index - 1].key];
                currentIndex = 0;
            }
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
                text: "<font color='" + modelData.color + "'>" + modelData.name + "</font>"
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
