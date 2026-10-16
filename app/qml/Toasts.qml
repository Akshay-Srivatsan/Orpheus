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
            padding: 10
            background: Rectangle {
                radius: 8
                color: palette.toolTipBase
                border.color: kind === "error" ? Theme.error : palette.highlight
            }
            RowLayout {
                anchors.fill: parent
                spacing: Theme.spacing
                Label { Layout.fillWidth: true; Layout.maximumWidth: 520; text: toast.text; wrapMode: Text.WordWrap; color: palette.toolTipText }
                Button {
                    visible: !!actionLabel
                    text: actionLabel
                    onClicked: Session.runToastAction(toastId)
                }
                ToolButton { text: "×"; onClicked: Session.dismiss(toastId) }
            }
        }
    }
}
