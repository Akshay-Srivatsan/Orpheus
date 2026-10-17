import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// Creates or edits a smart crate: a crate whose songs are whatever in the
// library matches its rules (e.g. "in Waltzes and tempo fits Rotary Waltz").
AppDialog {
    id: root
    width: 720
    title: editing ? qsTr("Edit Smart Crate") : qsTr("New Smart Crate")

    property var editing: null
    property var rules: ({ match: "all", conditions: [] })
    property var fields: []
    property var preview: null

    /// Opens it for crate `id` ("new": a new one).
    function edit(id) {
        editing = id === "new" ? null : Session.crates.find(c => c.id === id) || null;
        const style = Session.styleFilter.length === 1 && Session.styleFilter[0] !== "__none" ? Session.styleFilter[0] : null;
        nameField.text = editing ? editing.name : style ? qsTr("%1 (smart)").arg(Session.styleName(style)) : qsTr("New smart crate");
        Backend.call("smart_fields").then(f => {
            fields = f;
            if (editing && editing.smart) rules = JSON.parse(JSON.stringify(editing.smart));
            else if (style) rules = { match: "all", conditions: [{ field: "style", op: "is", value: style }] };
            else Backend.call("smart_default_condition", { field: "style" }).then(c => rules = { match: "all", conditions: [c] });
        });
        open();
    }

    function fieldDef(f) { return fields.find(d => d.field === f) || fields[0]; }
    function opDef(c) { const d = fieldDef(c.field); return d ? d.ops.find(o => o.op === c.op) : null; }
    function setRules(change) {
        const next = JSON.parse(JSON.stringify(rules));
        change(next);
        rules = next;
    }
    /// A sensible first value for an input kind.
    function firstValue(input, field) {
        switch (input) {
        case "style": return Session.myStyles.length ? Session.myStyles[0].key : null;
        case "crate": { const other = Session.crates.filter(c => !editing || c.id !== editing.id); return other.length ? other[0].id : null; }
        case "tag": return Session.myTags.length ? Session.myTags[0].key : null;
        }
        return undefined;
    }
    function setField(i, field) {
        Backend.call("smart_default_condition", { field: field }).then(c => {
            const op = fieldDef(field).ops.find(o => o.op === c.op);
            const v = firstValue(op ? op.input : "", field);
            if (v !== undefined) c.value = v;
            setRules(r => r.conditions[i] = c);
        });
    }
    function setOp(i, op) {
        setRules(r => {
            const c = r.conditions[i];
            const def = fieldDef(c.field).ops.find(o => o.op === op);
            c.op = op;
            if (!def) return;
            if (def.input === "range" && !Array.isArray(c.value)) c.value = [100, 120];
            if ((def.input === "number" || def.input === "days") && typeof c.value !== "number") c.value = c.field === "duration" ? 210 : 30;
            if (def.input === "style" && typeof c.value !== "string") c.value = firstValue("style");
            if (def.input === "source" && c.value !== "file" && c.value !== "spotify") c.value = "file";
            if (def.input === "none") c.value = null;
        });
    }

    onRulesChanged: previewLater.restart()
    Timer {
        id: previewLater
        interval: 150
        onTriggered: if (root.rules.conditions.length) Backend.call("smart_preview", { rules: root.rules, editing: root.editing ? root.editing.id : null }).then(p => root.preview = p)
    }

    standardButtons: Dialog.Cancel | Dialog.Save
    onAccepted: {
        if (editing) {
            if (nameField.text.trim() && nameField.text !== editing.name) Session.run("crate_rename", { id: editing.id, name: nameField.text.trim() });
            Session.run("crate_set_smart", { id: editing.id, smart: rules });
        } else {
            Session.run("crate_create", { name: nameField.text.trim() || qsTr("Smart crate"), smart: rules }).then(id => {
                if (!id) return;
                Session.show({ kind: "crate", id: id });
                Session.styleFilter = [];
            });
        }
    }

    ColumnLayout {
        width: parent.width
        spacing: Theme.spacing

        RowLayout {
            Layout.fillWidth: true
            Label { text: qsTr("Name") }
            TextField { id: nameField; Layout.fillWidth: true }
        }
        RowLayout {
            Label { text: qsTr("Songs matching") }
            ComboBox {
                implicitContentWidthPolicy: ComboBox.WidestText
                model: [qsTr("all"), qsTr("any")]
                currentIndex: root.rules.match === "any" ? 1 : 0
                onActivated: index => root.setRules(r => r.match = index === 1 ? "any" : "all")
            }
            Label { text: qsTr("of these rules:") }
        }
        Repeater {
            model: root.rules.conditions
            delegate: RowLayout {
                id: cond
                required property var modelData
                required property int index
                readonly property var op: root.opDef(modelData)
                Layout.fillWidth: true
                ComboBox {
                    Layout.preferredWidth: 200
                    model: root.fields.map(f => f.label)
                    currentIndex: root.fields.findIndex(f => f.field === cond.modelData.field)
                    onActivated: i => root.setField(cond.index, root.fields[i].field)
                }
                ComboBox {
                    Layout.preferredWidth: 190
                    readonly property var ops: root.fieldDef(cond.modelData.field) ? root.fieldDef(cond.modelData.field).ops : []
                    model: ops.map(o => o.label)
                    currentIndex: ops.findIndex(o => o.op === cond.modelData.op)
                    onActivated: i => root.setOp(cond.index, ops[i].op)
                }
                // The value, as the operator takes it.
                ComboBox {
                    visible: !!cond.op && (cond.op.input === "style" || cond.op.input === "crate" || cond.op.input === "tag" || cond.op.input === "source")
                    Layout.fillWidth: true
                    readonly property var choices: !cond.op ? []
                        : cond.op.input === "style" ? Session.myStyles.map(s => ({ id: s.key, label: s.name }))
                        : cond.op.input === "crate" ? Session.crates.filter(c => !root.editing || c.id !== root.editing.id).map(c => ({ id: c.id, label: c.name }))
                        : cond.op.input === "tag" ? Session.myTags.map(t => ({ id: t.key, label: t.name }))
                        : [{ id: "file", label: qsTr("Local file") }, { id: "spotify", label: qsTr("Spotify") }]
                    model: choices.map(c => c.label)
                    currentIndex: choices.findIndex(c => c.id === cond.modelData.value)
                    displayText: choices.length ? currentText
                        : cond.op && cond.op.input === "style" ? qsTr("No styles yet (Settings → Styles)")
                        : cond.op && cond.op.input === "tag" ? qsTr("No tags yet")
                        : qsTr("No other crates")
                    onActivated: i => root.setRules(r => r.conditions[cond.index].value = choices[i].id)
                }
                TextField {
                    visible: !!cond.op && (cond.op.input === "number" || cond.op.input === "days")
                    implicitWidth: 80
                    text: typeof cond.modelData.value === "number" ? String(cond.modelData.value) : ""
                    validator: DoubleValidator { bottom: 0 }
                    onEditingFinished: root.setRules(r => r.conditions[cond.index].value = Number(text))
                }
                Label {
                    visible: !!cond.op && cond.modelData.field === "duration" && typeof cond.modelData.value === "number"
                    text: Session.fmtTime(cond.modelData.value * 1000)
                    opacity: 0.6
                }
                TextField {
                    visible: !!cond.op && cond.op.input === "range"
                    implicitWidth: 70
                    text: Array.isArray(cond.modelData.value) ? String(cond.modelData.value[0]) : ""
                    onEditingFinished: root.setRules(r => r.conditions[cond.index].value = [Number(text), r.conditions[cond.index].value[1]])
                }
                Label { visible: !!cond.op && cond.op.input === "range"; text: "–" }
                TextField {
                    visible: !!cond.op && cond.op.input === "range"
                    implicitWidth: 70
                    text: Array.isArray(cond.modelData.value) ? String(cond.modelData.value[1]) : ""
                    onEditingFinished: root.setRules(r => r.conditions[cond.index].value = [r.conditions[cond.index].value[0], Number(text)])
                }
                TextField {
                    visible: !!cond.op && cond.op.input === "text"
                    Layout.fillWidth: true
                    text: typeof cond.modelData.value === "string" ? cond.modelData.value : ""
                    onTextEdited: root.setRules(r => r.conditions[cond.index].value = text)
                }
                Item { Layout.fillWidth: !!cond.op && (cond.op.input === "none" || cond.op.input === "number" || cond.op.input === "days" || cond.op.input === "range") }
                ToolButton {
                    icon.source: Theme.icon("minus")
                    enabled: root.rules.conditions.length > 1
                    ToolTip.visible: hovered; ToolTip.text: qsTr("Remove rule")
                    onClicked: root.setRules(r => r.conditions.splice(cond.index, 1))
                }
                ToolButton {
                    icon.source: Theme.icon("plus")
                    ToolTip.visible: hovered; ToolTip.text: qsTr("Add rule")
                    onClicked: Backend.call("smart_default_condition", { field: "bpm" }).then(c => {
                        const v = root.firstValue("style");
                        if (v !== undefined) c.value = v;
                        root.setRules(r => r.conditions.splice(cond.index + 1, 0, c));
                    })
                }
            }
        }
        Label {
            visible: !!root.preview
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            opacity: 0.75
            text: root.preview ? qsTr("%1 song(s) match now").arg(root.preview.count) + (root.preview.titles.length ? ": " + root.preview.titles.join(", ") + (root.preview.count > root.preview.titles.length ? " …" : "") : "") : ""
        }
    }
}
