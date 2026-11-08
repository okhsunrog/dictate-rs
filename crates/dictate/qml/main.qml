import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window
import org.kde.layershell 1.0 as LayerShell
import dictate

QtObject {
    id: app
    property Bridge bridge: Bridge {}
    property ApplicationWindow mainWindow: ApplicationWindow {
        id: win
        property bool empty: bridge.historyJson === "[]" && bridge.phase === 0
        property bool historyOpen: false
        width: 640; height: bridge.settingsVisible ? 640 : empty ? 320 : 560
        minimumWidth: 420; minimumHeight: 280
        title: bridge.settingsVisible ? qsTr("Dictate Settings") : qsTr("Dictate")
        visible: bridge.windowVisible
        color: palette.window
        onClosing: close => { close.accepted = false; bridge.teardown() }

        property Connections updates: Connections {
            target: bridge
            function refreshEditor() {
                let next = bridge.showingOriginal ? bridge.originalText : bridge.editedText
                if (editor.text !== next) {
                    editor.syncing = true
                    editor.text = next
                    editor.syncing = false
                }
            }
            function onEditedTextChanged() { refreshEditor() }
            function onOriginalTextChanged() { refreshEditor() }
            function onShowingOriginalChanged() { refreshEditor() }
        }
        Shortcut { sequence: "Ctrl+Return"; enabled: !bridge.settingsVisible && editor.text.trim().length > 0; onActivated: bridge.copyAndClose() }
        Shortcut { sequence: "Escape"; onActivated: bridge.windowVisible = false }
        header: ToolBar {
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 16; anchors.rightMargin: 12
                Label { text: bridge.settingsVisible ? qsTr("Settings") : qsTr("Dictate"); font.bold: true; font.pointSize: 14; Layout.fillWidth: true }
                ToolButton {
                    text: bridge.settingsVisible ? qsTr("Back to text") : qsTr("Settings")
                    icon.name: bridge.settingsVisible ? "go-previous" : "configure"
                    onClicked: bridge.settingsVisible = !bridge.settingsVisible
                }
                ToolButton {
                    text: qsTr("More"); icon.name: "application-menu"
                    onClicked: appMenu.popup()
                    Menu {
                        id: appMenu
                        MenuItem { text: qsTr("History"); enabled: JSON.parse(bridge.historyJson).length > 1; checkable: true; checked: win.historyOpen; onTriggered: win.historyOpen = !win.historyOpen }
                        MenuItem { text: qsTr("Original transcript"); enabled: bridge.originalText.length > 0; checkable: true; checked: bridge.showingOriginal; onTriggered: bridge.toggleOriginal() }
                        MenuItem { text: bridge.originalText.length ? qsTr("Retry editing") : qsTr("Retry transcription"); enabled: bridge.canRetry && bridge.phase === 0; onTriggered: bridge.retry() }
                        MenuSeparator {}
                        MenuItem { text: qsTr("Quit"); onTriggered: bridge.teardown() }
                    }
                }
            }
        }
        StackLayout {
            anchors.fill: parent; anchors.margins: 20
            currentIndex: bridge.settingsVisible ? 1 : 0
            ColumnLayout {
                spacing: 12
                ComboBox {
                    Layout.fillWidth: true
                    model: JSON.parse(bridge.historyJson)
                    currentIndex: bridge.historyIndex
                    visible: win.historyOpen && count > 1
                    enabled: bridge.phase === 0
                    onActivated: bridge.selectSession(currentIndex)
                    Accessible.name: qsTr("Recent dictations, in memory only")
                }
                RowLayout {
                    visible: !win.empty
                    Layout.fillWidth: true
                    BusyIndicator { running: bridge.phase > 1; visible: running; implicitWidth: 24; implicitHeight: 24 }
                    Label { text: bridge.statusText; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                    ToolButton { text: qsTr("Stop"); visible: bridge.phase > 1; onClicked: bridge.cancel() }
                }
                Frame {
                    visible: bridge.errorText.length > 0
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        Label { text: bridge.errorText; wrapMode: Text.WordWrap; Layout.fillWidth: true; Accessible.role: Accessible.AlertMessage }
                        Flow {
                            Layout.fillWidth: true; spacing: 8
                            Button { text: qsTr("Retry"); visible: !bridge.pasteError; enabled: bridge.canRetry && bridge.phase === 0; onClicked: bridge.retry() }
                            Button { text: qsTr("Configure connection"); onClicked: bridge.settingsVisible = true }
                        }
                    }
                }
                ColumnLayout {
                    visible: win.empty
                    Layout.fillWidth: true; Layout.fillHeight: true
                    spacing: 16
                    Item { Layout.fillHeight: true }
                    Label { text: qsTr("Turn speech into text"); font.pointSize: 18; font.bold: true; Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter }
                    Label { text: qsTr("Record, review and copy."); Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter }
                    Button { text: qsTr("Start recording"); highlighted: true; Layout.alignment: Qt.AlignHCenter; onClicked: bridge.toggle() }
                    Item { Layout.fillHeight: true }
                }
                Label { visible: bridge.showingOriginal; text: qsTr("Original transcript · read-only") }
                ScrollView {
                    visible: !win.empty
                    Layout.fillWidth: true; Layout.fillHeight: true
                    clip: true
                    TextArea {
                        id: editor
                        property bool syncing: false
                        readOnly: bridge.showingOriginal || bridge.historyJson === "[]"
                        wrapMode: TextArea.Wrap
                        padding: 16
                        selectByMouse: true
                        font.pointSize: 12
                        placeholderText: bridge.phase > 1 ? qsTr("Transcribing recording…") : qsTr("Dictation text")
                        onTextChanged: if (!syncing && !bridge.showingOriginal) bridge.editDraft(text)
                        Accessible.name: bridge.showingOriginal ? qsTr("Original transcript") : qsTr("Dictation text")
                    }
                }
                RowLayout {
                    visible: !win.empty
                    Layout.fillWidth: true; spacing: 8
                    Button { text: qsTr("Copy"); highlighted: true; enabled: editor.text.trim().length > 0; onClicked: bridge.copyCurrent() }
                    Button { text: bridge.phase === 1 ? qsTr("Finish") : qsTr("New recording"); enabled: bridge.phase <= 1; onClicked: bridge.toggle() }
                    Item { Layout.fillWidth: true }
                }
            }
            ScrollView {
                clip: true
                contentWidth: availableWidth
                ColumnLayout {
                    width: parent.width; spacing: 10
                    Label { text: qsTr("Transcription"); font.bold: true }
                    ComboBox {
                        Layout.fillWidth: true
                        model: [qsTr("ChatGPT proxy"), qsTr("ElevenLabs · Scribe v2")]
                        currentIndex: bridge.cfgSttProvider
                        onActivated: bridge.cfgSttProvider = currentIndex
                        Accessible.name: qsTr("Transcription provider")
                    }
                    Label { visible: bridge.cfgSttProvider === 1; text: qsTr("ElevenLabs API key") }
                    TextField {
                        visible: bridge.cfgSttProvider === 1
                        Layout.fillWidth: true
                        echoMode: TextInput.Password
                        text: bridge.cfgElevenlabsApiKey
                        onTextEdited: bridge.cfgElevenlabsApiKey = text
                        Accessible.name: qsTr("ElevenLabs API key")
                    }
                    Label {
                        visible: bridge.cfgSttProvider === 1
                        text: qsTr("ELEVENLABS_API_KEY overrides the saved key. A key entered here is stored locally with mode 0600.")
                        wrapMode: Text.WordWrap; Layout.fillWidth: true; opacity: 0.7
                    }
                    Label { text: qsTr("Editing"); font.bold: true }
                    Label { visible: bridge.cfgSttProvider === 0; text: qsTr("This service also handles transcription."); wrapMode: Text.WordWrap; Layout.fillWidth: true }
                    Label { text: qsTr("Service URL (including /v1)") }
                    TextField { Layout.fillWidth: true; text: bridge.cfgBaseUrl; onTextEdited: bridge.cfgBaseUrl = text; Accessible.name: qsTr("Service URL") }
                    Label { text: qsTr("API key") }
                    TextField { Layout.fillWidth: true; echoMode: TextInput.Password; text: bridge.cfgApiKey; onTextEdited: bridge.cfgApiKey = text; Accessible.name: qsTr("API key") }
                    Label { text: qsTr("DICTATE_API_KEY overrides the saved key. A key entered here is stored locally with mode 0600."); wrapMode: Text.WordWrap; Layout.fillWidth: true; opacity: 0.7 }
                    Label { text: qsTr("Automatic paste"); font.bold: true }
                    Label { text: qsTr("Text is pasted into the application focused when processing finishes."); wrapMode: Text.WordWrap; Layout.fillWidth: true }
                    CheckBox { text: qsTr("Terminal compatibility"); checked: bridge.cfgPasteShift; onToggled: bridge.cfgPasteShift = checked }
                    Label { text: bridge.pasteError ? qsTr("Failed to connect automatic paste. See details below.") : bridge.pasteReady ? (bridge.pasteSaved ? qsTr("Automatic paste connected. Permission saved.") : qsTr("Connected for this session. The portal did not allow access to be saved.")) : bridge.pasteSaved ? qsTr("Permission saved. The connection will be restored when pasting.") : qsTr("Allow automatic paste once for future launches."); wrapMode: Text.WordWrap; Layout.fillWidth: true }
                    Button { text: bridge.pasteReady ? qsTr("Automatic paste connected") : bridge.pasteSaved ? qsTr("Check connection") : qsTr("Connect automatic paste"); enabled: !bridge.pasteReady; onClicked: bridge.connectPaste() }
                    Label { text: qsTr("Advanced settings"); font.bold: true }
                    Label { text: qsTr("Language (empty for automatic detection)") }
                    TextField { Layout.fillWidth: true; text: bridge.cfgLanguage; placeholderText: "ru, en"; onTextEdited: bridge.cfgLanguage = text }
                    Label { text: qsTr("Editing model") }
                    TextField { Layout.fillWidth: true; text: bridge.cfgModel; onTextEdited: bridge.cfgModel = text }
                    Label { text: qsTr("PipeWire microphone (empty for default)") }
                    TextField { Layout.fillWidth: true; text: bridge.cfgMicrophone; onTextEdited: bridge.cfgMicrophone = text }
                    Button { text: qsTr("Save settings"); enabled: bridge.phase === 0; onClicked: bridge.saveSettings() }
                    Label { text: bridge.settingsStatus; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                    Button { text: qsTr("Resume processing"); visible: bridge.canRetry; enabled: bridge.phase === 0; onClicked: bridge.retry() }
                    Label { text: qsTr("The last 10 drafts are kept until exit. Audio is retained for retry until the next recording."); wrapMode: Text.WordWrap; Layout.fillWidth: true; opacity: 0.7 }
                }
            }
        }
    }
    property Window osd: Window {
        id: osdWin
        width: 440; height: osdContent.implicitHeight + 28
        visible: bridge.osdVisible
        color: palette.window
        flags: Qt.FramelessWindowHint
        LayerShell.Window.anchors: LayerShell.Window.AnchorBottom
        LayerShell.Window.layer: LayerShell.Window.LayerOverlay
        LayerShell.Window.keyboardInteractivity: LayerShell.Window.KeyboardInteractivityNone
        LayerShell.Window.exclusionZone: 0
        LayerShell.Window.wantsToBeOnActiveScreen: true
        LayerShell.Window.scope: "dictate"
        ColumnLayout {
            id: osdContent
            anchors.fill: parent; anchors.margins: 14; spacing: 8
            RowLayout {
                Label { text: bridge.phase === 1 ? "●" : "✓"; color: bridge.phase === 1 ? "#da4453" : palette.text; font.pointSize: 15 }
                Label {
                    text: bridge.phase === 1 ? qsTr("Recording · %1:%2").arg(Math.floor(bridge.seconds / 60)).arg(("0" + bridge.seconds % 60).slice(-2)) : bridge.errorText.length ? qsTr("Failed to start recording") : bridge.statusText
                    Layout.fillWidth: true; wrapMode: Text.WordWrap
                }
                Button { text: qsTr("Finish"); visible: bridge.phase === 1; onClicked: bridge.toggle() }
                ToolButton { text: bridge.phase === 0 ? qsTr("Close") : qsTr("Cancel"); onClicked: { if (bridge.phase === 0) bridge.osdVisible = false; else bridge.cancel() } }
            }
            ProgressBar { visible: bridge.phase === 1; Layout.fillWidth: true; value: bridge.level }
            Label { visible: bridge.errorText.length > 0; text: bridge.errorText; wrapMode: Text.WordWrap; Layout.fillWidth: true }
        }
        Timer { interval: 1800; running: bridge.osdVisible && bridge.phase === 0 && bridge.errorText.length === 0; onTriggered: bridge.osdVisible = false }
    }
    Component.onCompleted: {
        Qt.application.domain = "okhsunrog.ru"
        bridge.initialize()
    }
}
