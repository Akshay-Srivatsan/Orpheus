import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// The window's status bar: the latest message (errors stay longer; some have a
// button, e.g. Undo), and what the library is doing.
ToolBar {
    id: root
    position: ToolBar.Footer

    readonly property int count: Session.toasts.count
    // The newest message (the model changes as messages come and go).
    readonly property var latest: count > 0 ? Session.toasts.get(count - 1) : null
    /// The recordings being analysed (BPM, meter, loudness, …).
    readonly property var analyzing: Session.analysis ? Session.analysis.current : []

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: Theme.spacing
        anchors.rightMargin: Theme.spacing / 2
        spacing: Theme.spacing

        Icon {
            visible: !!root.latest
            glyph: root.latest && root.latest.kind === "error" ? "warning" : "info"
            color: root.latest && root.latest.kind === "error" ? Theme.error : Theme.system.text
        }
        Label {
            Layout.fillWidth: true
            elide: Text.ElideRight
            text: root.latest ? root.latest.text
                : Session.scan && Session.scan.running ? Session.scan.phase + (Session.scan.total ? " " + Session.scan.done + "/" + Session.scan.total : "")
                : root.analyzing.length ? qsTr("Analyzing %1…").arg(root.analyzing.map(c => "“" + c.title + "”").join(", "))
                    + (Session.analysis.waiting ? " " + qsTr("(%n more waiting)", "", Session.analysis.waiting) : "")
                : Session.summary ? qsTr("%n song(s) in the library", "", Session.summary.songs) : ""
            color: root.latest && root.latest.kind === "error" ? Theme.error : Theme.system.text
            opacity: root.latest ? 1 : Theme.dim
            ToolTip.visible: messageHover.hovered && truncated
            ToolTip.text: text
            HoverHandler { id: messageHover }
        }
        Label {
            visible: root.count > 1
            text: qsTr("+%1 more").arg(root.count - 1)
            opacity: Theme.dim
        }
        Button {
            // (Its label is the message's: there's none without one.)
            visible: !!root.latest && !!root.latest.actionLabel
            text: root.latest ? root.latest.actionLabel : ""
            onClicked: Session.runToastAction(root.latest.toastId)
        }
        ToolButton {
            icon.color: palette.buttonText
            enabled: !!root.latest
            icon.source: Theme.icon("x")
            icon.width: Theme.smallIconSize; icon.height: Theme.smallIconSize
            ToolTip.visible: hovered; ToolTip.text: qsTr("Dismiss")
            onClicked: Session.dismiss(root.latest.toastId)
        }
    }
}
