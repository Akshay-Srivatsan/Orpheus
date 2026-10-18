import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

SettingsPage {
    id: page
    title: qsTr("Spotify")
    intro: qsTr("Play songs from Spotify (Premium required) alongside your local files, with the same tempo control, fades and preview. Spotify needs an internet connection while playing.")

    readonly property var s: Session.settings
    readonly property var sp: Session.spotify
    property var cache: null
    Component.onCompleted: Backend.call("spotify_cache_bytes").then(b => cache = b)
    function setSpotify(patch) { Session.saveSettings({ spotify: Object.assign({}, s.spotify, patch) }); }

    GridLayout {
        visible: !!page.s
        Layout.fillWidth: true
        columns: 2
        columnSpacing: Theme.spacing * 2
        rowSpacing: Theme.spacing

        FormLabel { text: qsTr("Account"); Layout.alignment: Qt.AlignTop }
        ColumnLayout {
            Label { visible: !!page.sp && page.sp.connected; textFormat: Text.StyledText; text: page.sp ? qsTr("Connected as <b>%1</b>").arg(page.sp.username || "") : "" }
            Label {
                visible: !!page.sp && page.sp.connected && !page.sp.webApi && !page.sp.ownWebApp
                color: Theme.warn
                text: qsTr("Sign in once more to enable Spotify search and your playlists.")
            }
            Button {
                enabled: !!page.sp && page.sp.connected && !page.sp.webApi && !page.sp.ownWebApp
                highlighted: true
                text: qsTr("Sign In Again…")
                onClicked: Session.run("spotify_login")
            }
            RowLayout {
                visible: !!page.sp && page.sp.connected
                Button {
                    text: qsTr("Refresh Session")
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Reconnect and renew the sign-in now (e.g. just before a set). Orpheus also reconnects by itself when Spotify drops the session.")
                    onClicked: Session.run("spotify_refresh").then(r => { if (r) Session.toast(qsTr("Spotify session refreshed."), "info"); })
                }
                Button { text: qsTr("Disconnect"); onClicked: Session.run("spotify_logout") }
            }
            RowLayout {
                visible: !page.sp || !page.sp.connected
                Button {
                    highlighted: true
                    enabled: !(page.sp && page.sp.connecting)
                    text: page.sp && page.sp.connecting ? qsTr("Waiting for browser…") : qsTr("Connect Spotify…")
                    onClicked: Session.run("spotify_login")
                }
                Button { enabled: !!page.sp && page.sp.hasCachedCredentials; text: qsTr("Reconnect"); onClicked: Session.run("spotify_connect_cached") }
            }
            Label { visible: !page.sp || !page.sp.connected; text: qsTr("Opens your browser to sign in."); font.pointSize: Theme.smallPointSize; opacity: 0.7 }
            Label { visible: !!page.sp && !!page.sp.error; text: page.sp && page.sp.error ? page.sp.error : ""; color: Theme.error }
        }

        FormLabel { text: qsTr("Downloaded audio") }
        RowLayout {
            Label { text: page.cache === null ? "…" : qsTr("%1 MB").arg((page.cache / 1e6).toFixed(0)) }
            Button {
                text: qsTr("Clear Cache…")
                enabled: !!page.cache
                onClicked: Session.confirm(qsTr("Delete Spotify audio cached on this computer? It is downloaded again when needed."), qsTr("Delete"),
                    ok => { if (ok) Session.run("spotify_clear_cache").then(() => Backend.call("spotify_cache_bytes").then(b => page.cache = b)); })
            }
        }

        FormLabel { text: qsTr("Startup") }
        CheckBox { text: qsTr("Reconnect automatically"); checked: !!page.s && page.s.spotify.autoConnect; onToggled: page.setSpotify({ autoConnect: checked }) }

        FormLabel { text: qsTr("Search app"); Layout.alignment: Qt.AlignTop }
        ColumnLayout {
            Layout.fillWidth: true
            TextField {
                Layout.preferredWidth: 320
                placeholderText: qsTr("Shared (default)")
                text: page.s ? page.s.spotify.clientId : ""
                onEditingFinished: page.setSpotify({ clientId: text.trim() })
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                font.pointSize: Theme.smallPointSize
                opacity: 0.7
                text: qsTr("Search and your playlists use a client ID shared by many apps, which Spotify rate-limits. If you get “Spotify is limiting requests”, enter the client ID of your own app from developer.spotify.com (Dashboard → Create app, with redirect URI http://127.0.0.1:8898/login and the Web API). Playback keeps using your normal sign-in.")
            }
            Label { visible: !!page.sp && page.sp.ownWebApp && page.sp.webApi; text: qsTr("Search uses your app.") }
            Button {
                enabled: !!page.sp && page.sp.ownWebApp && !page.sp.webApi
                highlighted: true
                text: qsTr("Sign In for Search…")
                onClicked: Session.run("spotify_web_login")
            }
        }
    }
}
