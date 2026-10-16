pragma Singleton
import QtQuick
import Orpheus

// The engine for the interface: `call` returns a promise of the command's
// result; engine events are re-sent as `event(name, payload)`.
QtObject {
    id: backend

    signal event(string name, var payload)

    readonly property string failure: Engine.failure

    property var pending: ({})

    function call(name, args) {
        return new Promise((resolve, reject) => {
            const id = Engine.call(name, args === undefined ? "" : JSON.stringify(args));
            pending[id] = { resolve, reject };
        });
    }

    readonly property Connections connections: Connections {
        target: Engine
        function onReplied(id, ok, result) {
            const p = backend.pending[id];
            if (!p) return;
            delete backend.pending[id];
            if (ok) p.resolve(result === "" ? null : JSON.parse(result));
            else p.reject(result);
        }
        function onNotified(name, payload) {
            backend.event(name, payload === "" ? null : JSON.parse(payload));
        }
    }
}
