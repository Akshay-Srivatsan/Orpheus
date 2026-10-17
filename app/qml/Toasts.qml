import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// Messages at the bottom of the window (errors stay longer; some have a button).
ColumnLayout {
    spacing: 6

    Repeater {
        model: Session.toasts
        delegate: Pane {
            id: toast
            required property int toastId
            required property string text
            required property string kind
            required property string actionLabel
            Layout.alignment: Qt.AlignHCenter
            Layout.maximumWidth: 640
            padding: 6
            leftPadding: 12
            background: Rectangle {
                radius: 4
                color: Theme.system.window
                border.color: toast.kind === "error" ? Theme.error : Theme.system.mid
                Rectangle {
                    anchors.fill: parent
                    radius: parent.radius
                    color: toast.kind === "error" ? Qt.alpha(Theme.error, 0.1) : Qt.alpha(Theme.system.highlight, 0.08)
                }
            }
            RowLayout {
                anchors.fill: parent
                spacing: Theme.spacing
                Icon {
                    glyph: toast.kind === "error" ? "warning" : "info"
                    color: toast.kind === "error" ? Theme.error : Theme.system.highlight
                }
                Label { Layout.fillWidth: true; Layout.maximumWidth: 520; text: toast.text; wrapMode: Text.WordWrap }
                Button {
                    visible: !!toast.actionLabel
                    text: toast.actionLabel
                    onClicked: Session.runToastAction(toast.toastId)
                }
                ToolButton {
                    icon.source: Theme.icon("x")
                    icon.width: Theme.smallIconSize; icon.height: Theme.smallIconSize
                    ToolTip.visible: hovered; ToolTip.text: qsTr("Dismiss")
                    onClicked: Session.dismiss(toast.toastId)
                }
            }
        }
    }
}
