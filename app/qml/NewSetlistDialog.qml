import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// A new setlist: its name, date, club and whether it's planned or built live.
AppDialog {
    id: root
    title: qsTr("New Setlist")
    standardButtons: Dialog.Ok | Dialog.Cancel

    readonly property string today: Qt.formatDate(new Date(), "yyyy-MM-dd")
    readonly property var clubChoice: club.currentIndex > 0 ? Session.clubs[club.currentIndex - 1] : null
    readonly property string defaultName: (clubChoice ? clubChoice.name : qsTr("Social")) + " — " + date.text

    onAboutToShow: {
        name.text = "";
        date.text = today;
        preset.checked = true;
        const i = Session.settings ? Session.clubs.findIndex(c => c.id === Session.settings.defaultClub) : -1;
        club.currentIndex = i + 1;
        name.forceActiveFocus();
    }
    onAccepted: {
        Session.run("setlist_create", {
            name: name.text.trim() || defaultName,
            mode: preset.checked ? "preset" : "live",
            clubId: clubChoice ? clubChoice.id : null,
            eventDate: date.text || null,
        }).then(set => {
            if (!set) return;
            Session.run("perform_open", { setlistId: set.id });
            Session.rightTab = "set";
        });
    }

    GridLayout {
        columns: 2
        columnSpacing: Theme.spacing * 2
        rowSpacing: Theme.spacing

        FormLabel { text: qsTr("Name") }
        TextField { id: name; Layout.preferredWidth: 320; placeholderText: root.defaultName; onAccepted: root.accept() }
        FormLabel { text: qsTr("Date") }
        TextField { id: date; Layout.preferredWidth: 160; inputMask: "9999-99-99"; placeholderText: "YYYY-MM-DD" }
        FormLabel { text: qsTr("Club") }
        ColumnLayout {
            ComboBox {
                id: club
                Layout.preferredWidth: 320
                model: [qsTr("No club (your own styles)")].concat(Session.clubs.map(c => c.name))
            }
            Label {
                Layout.preferredWidth: 320
                wrapMode: Text.WordWrap
                font.pointSize: Theme.smallPointSize
                opacity: 0.7
                text: qsTr("The set keeps the club's current guidelines, even if the club publishes new ones (you can move it later).")
                    + (Session.clubs.length ? "" : " " + qsTr("Add clubs in Settings → Clubs."))
            }
        }
        FormLabel { text: qsTr("Kind") }
        ColumnLayout {
            RadioButton { id: preset; text: qsTr("Preset: plan the songs ahead; the projector shows the whole setlist") }
            RadioButton { text: qsTr("Live: pick as you go; the projector shows what's playing and recent songs") }
            Label {
                opacity: Theme.dim
                font.pointSize: Theme.smallPointSize
                text: qsTr("You can mix both: add style slots now and choose songs during the event.")
            }
        }
    }
}
