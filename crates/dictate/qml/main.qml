import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Basic as Basic
import QtQuick.Layouts
import QtQuick.Window
import org.kde.layershell 1.0 as LayerShell
import dictate

QtObject {
    id: app
    property Bridge bridge: Bridge {}
    property SystemPalette systemColors: SystemPalette { colorGroup: SystemPalette.Active }
    readonly property bool dark: systemColors.window.hslLightness < 0.5
    readonly property color canvas: dark ? "#19212e" : "#f3f6fa"
    readonly property color surface: dark ? "#202b3b" : "#ffffff"
    readonly property color hover: dark ? "#2a374a" : "#e7edf5"
    readonly property color ink: dark ? "#e8eef8" : "#202c40"
    readonly property color muted: dark ? "#9aaac0" : "#60718a"
    readonly property color line: dark ? "#344258" : "#d5dfeb"
    readonly property color accent: dark ? "#9dbfff" : "#315fbd"
    readonly property color accentInk: dark ? "#152c52" : "#ffffff"
    readonly property color accentSoft: dark ? "#2a3d5b" : "#e4edff"
    readonly property color danger: dark ? "#ffb4ae" : "#af3434"
    readonly property color dangerSoft: dark ? "#3c2b35" : "#fff0ee"

    readonly property var languages: [
        { code: "", label: "Automatic detection" },
        { code: "ar", label: "Arabic" }, { code: "bg", label: "Bulgarian" },
        { code: "ca", label: "Catalan" }, { code: "zh", label: "Chinese" },
        { code: "hr", label: "Croatian" }, { code: "cs", label: "Czech" },
        { code: "da", label: "Danish" }, { code: "nl", label: "Dutch" },
        { code: "en", label: "English" }, { code: "et", label: "Estonian" },
        { code: "fi", label: "Finnish" }, { code: "fr", label: "French" },
        { code: "de", label: "German" }, { code: "el", label: "Greek" },
        { code: "he", label: "Hebrew" }, { code: "hi", label: "Hindi" },
        { code: "hu", label: "Hungarian" }, { code: "id", label: "Indonesian" },
        { code: "it", label: "Italian" }, { code: "ja", label: "Japanese" },
        { code: "ko", label: "Korean" }, { code: "lv", label: "Latvian" },
        { code: "lt", label: "Lithuanian" }, { code: "ms", label: "Malay" },
        { code: "no", label: "Norwegian" }, { code: "fa", label: "Persian" },
        { code: "pl", label: "Polish" }, { code: "pt", label: "Portuguese" },
        { code: "ro", label: "Romanian" }, { code: "ru", label: "Russian" },
        { code: "sr", label: "Serbian" }, { code: "sk", label: "Slovak" },
        { code: "sl", label: "Slovenian" }, { code: "es", label: "Spanish" },
        { code: "sv", label: "Swedish" }, { code: "ta", label: "Tamil" },
        { code: "th", label: "Thai" }, { code: "tr", label: "Turkish" },
        { code: "uk", label: "Ukrainian" }, { code: "vi", label: "Vietnamese" }
    ]
    readonly property var languageOptions: {
        let options = languages.slice()
        if (!options.some(item => item.code === bridge.cfgLanguage))
            options.push({ code: bridge.cfgLanguage, label: bridge.cfgLanguage })
        return options
    }
    readonly property var microphoneInfo: JSON.parse(bridge.microphonesJson)
    readonly property var microphoneOptions: {
        let options = microphoneInfo.devices.slice()
        if (!options.some(item => item.name === bridge.cfgMicrophone))
            options.push({ name: bridge.cfgMicrophone, label: qsTr("Saved microphone (disconnected)") })
        return options
    }

    component Glyph: Canvas {
        property string name: "mic"
        property color tint: app.ink
        implicitWidth: 20; implicitHeight: 20
        onNameChanged: requestPaint()
        onTintChanged: requestPaint()
        onPaint: {
            const c = getContext("2d")
            c.reset(); c.scale(width / 24, height / 24)
            c.strokeStyle = tint; c.lineWidth = 1.7; c.lineCap = "round"; c.lineJoin = "round"
            c.beginPath()
            switch (name) {
            case "mic":
                c.roundedRect(9, 3, 6, 11, 3, 3)
                c.moveTo(5.5, 11); c.lineTo(5.5, 12); c.bezierCurveTo(5.5, 21, 18.5, 21, 18.5, 12); c.lineTo(18.5, 11)
                c.moveTo(12, 19); c.lineTo(12, 22); c.moveTo(9, 22); c.lineTo(15, 22); break
            case "settings":
                c.moveTo(4, 7); c.lineTo(20, 7); c.moveTo(4, 17); c.lineTo(20, 17); c.stroke()
                c.beginPath(); c.fillStyle = app.canvas; c.arc(9, 7, 3, 0, Math.PI * 2); c.fill(); c.stroke()
                c.beginPath(); c.arc(15, 17, 3, 0, Math.PI * 2); c.fill(); break
            case "expand":
                c.moveTo(4, 10); c.lineTo(4, 4); c.lineTo(10, 4)
                c.moveTo(14, 4); c.lineTo(20, 4); c.lineTo(20, 10)
                c.moveTo(20, 14); c.lineTo(20, 20); c.lineTo(14, 20)
                c.moveTo(10, 20); c.lineTo(4, 20); c.lineTo(4, 14); break
            case "compact": c.roundedRect(3, 10, 18, 8, 3, 3); break
            case "copy": c.roundedRect(8, 8, 12, 13, 2, 2); c.moveTo(15, 4); c.lineTo(4, 4); c.lineTo(4, 16); break
            case "back": c.moveTo(13, 5); c.lineTo(6, 12); c.lineTo(13, 19); c.moveTo(6, 12); c.lineTo(21, 12); break
            case "close": c.moveTo(6, 6); c.lineTo(18, 18); c.moveTo(18, 6); c.lineTo(6, 18); break
            case "stop": c.roundedRect(6, 6, 12, 12, 2, 2); break
            case "check": c.moveTo(5, 12); c.lineTo(10, 17); c.lineTo(20, 6); break
            case "chevron": c.moveTo(6, 9); c.lineTo(12, 15); c.lineTo(18, 9); break
            case "history":
                c.arc(12, 12, 8, -Math.PI * 0.8, Math.PI * 0.9)
                c.moveTo(3, 4); c.lineTo(3, 9); c.lineTo(8, 9)
                c.moveTo(12, 7); c.lineTo(12, 12); c.lineTo(16, 14); break
            }
            c.stroke()
        }
    }
    component WaveMark: Row {
        property color tint: app.accent
        spacing: 3
        Repeater {
            model: [12, 23, 32, 20, 10]
            Rectangle { required property int modelData; width: 4; height: modelData; radius: 2; y: (32 - height) / 2; color: tint }
        }
        height: 32; width: 32
    }
    component Action: Basic.Button {
        id: action
        property bool primary: false
        property bool quiet: false
        property string glyph: ""
        hoverEnabled: true
        ToolTip.visible: hovered && text.length === 0
        ToolTip.text: Accessible.name
        ToolTip.delay: 600
        implicitHeight: 40
        implicitWidth: Math.max(40, contentItem.implicitWidth + (text.length ? 28 : 20))
        padding: 10
        opacity: enabled ? 1 : 0.42
        font.pixelSize: 13; font.weight: Font.Medium
        background: Rectangle {
            radius: 9
            color: action.down ? (action.primary ? Qt.darker(app.accent, 1.15) : app.hover) : action.primary ? app.accent : action.hovered ? app.hover : action.quiet ? "transparent" : app.surface
            border.width: action.visualFocus ? 2 : action.quiet || action.primary ? 0 : 1
            border.color: action.visualFocus ? (action.primary ? app.ink : app.accent) : app.line
        }
        contentItem: Row {
            spacing: 8
            anchors.centerIn: parent
            Glyph { visible: action.glyph.length > 0; name: action.glyph; tint: action.primary ? app.accentInk : app.ink; anchors.verticalCenter: parent.verticalCenter }
            Text { visible: action.text.length > 0; text: action.text; font: action.font; color: action.primary ? app.accentInk : app.ink; anchors.verticalCenter: parent.verticalCenter }
        }
    }
    component Field: Basic.TextField {
        id: field
        implicitHeight: 42
        color: app.ink; placeholderTextColor: app.muted
        selectionColor: app.accent; selectedTextColor: app.accentInk
        font.pixelSize: 14
        leftPadding: 12; rightPadding: 12
        selectByMouse: true
        background: Rectangle { radius: 8; color: app.surface; border.color: field.activeFocus ? app.accent : app.line; border.width: field.activeFocus ? 2 : 1 }
    }
    component ScrollRail: Basic.ScrollBar {
        id: rail
        orientation: Qt.Vertical
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.right: parent.right
        implicitWidth: 12
        implicitHeight: 40
        padding: 3
        minimumSize: 0.12
        hoverEnabled: true
        policy: ScrollBar.AsNeeded
        contentItem: Rectangle {
            implicitWidth: 6; implicitHeight: 6; radius: 3
            color: rail.pressed ? app.accent : rail.hovered ? app.muted : app.line
        }
        background: Item {}
    }
    component Choice: Basic.ComboBox {
        id: choice
        implicitHeight: 42
        leftPadding: 12; rightPadding: 36
        font.pixelSize: 14
        contentItem: Basic.TextField {
            text: choice.editable ? choice.editText : choice.displayText
            readOnly: !choice.editable
            selectByMouse: choice.editable
            autoScroll: choice.editable
            padding: 0
            font: choice.font; color: app.ink
            selectionColor: app.accent; selectedTextColor: app.accentInk
            verticalAlignment: Text.AlignVCenter
            background: null
            onEditingFinished: if (choice.editable) choice.accepted()
        }
        indicator: Glyph { name: "chevron"; tint: app.muted; x: choice.width - width - 12; y: (choice.height - height) / 2 }
        background: Rectangle { radius: 8; color: app.surface; border.color: choice.visualFocus ? app.accent : app.line; border.width: choice.visualFocus ? 2 : 1 }
        delegate: Basic.ItemDelegate {
            required property var modelData
            required property int index
            width: choice.width
            implicitHeight: 40
            highlighted: choice.highlightedIndex === index
            contentItem: Text { text: choice.textRole ? modelData[choice.textRole] : modelData; color: app.ink; font: choice.font; verticalAlignment: Text.AlignVCenter; elide: Text.ElideRight }
            background: Rectangle { color: parent.highlighted ? app.hover : app.surface; radius: 6 }
        }
        popup: Basic.Popup {
            y: choice.height + 5; width: choice.width; padding: 5
            implicitHeight: Math.min(contentItem.implicitHeight + 10, 220)
            background: Rectangle { color: app.surface; border.color: app.line; radius: 10 }
            contentItem: ListView {
                clip: true; implicitHeight: contentHeight
                model: choice.popup.visible ? choice.delegateModel : null
                currentIndex: choice.highlightedIndex
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollRail {}
            }
        }
    }
    component Caption: Text {
        color: app.muted; font.pixelSize: 12; wrapMode: Text.WordWrap
    }
    component FieldLabel: Text {
        color: app.ink; font.pixelSize: 13; font.weight: Font.Medium
    }
    component Section: ColumnLayout {
        property string title: ""
        property string description: ""
        spacing: 5
        Layout.fillWidth: true
        Layout.topMargin: 12
        Text { text: parent.title; color: app.ink; font.pixelSize: 17; font.weight: Font.DemiBold }
        Caption { text: parent.description; visible: text.length > 0; Layout.fillWidth: true }
    }
    component Rule: Rectangle { color: app.line; implicitHeight: 1; Layout.fillWidth: true }

    property ApplicationWindow mainWindow: ApplicationWindow {
        id: win
        property bool empty: bridge.historyJson === "[]" && bridge.phase === 0
        property bool historyOpen: false
        width: 640; height: bridge.settingsVisible ? 640 : empty ? 400 : 560
        minimumWidth: 420; minimumHeight: 320
        title: bridge.settingsVisible ? qsTr("Dictate Settings") : qsTr("Dictate")
        visible: bridge.windowVisible
        color: app.canvas
        font.family: "Noto Sans"
        palette.text: app.ink
        palette.windowText: app.ink
        palette.buttonText: app.ink
        palette.highlight: app.accent
        palette.highlightedText: app.accentInk
        onClosing: close => { close.accepted = false; bridge.teardown() }
        property Connections updates: Connections {
            target: bridge
            function refreshEditor() {
                let next = bridge.showingOriginal ? bridge.originalText : bridge.editedText
                if (editor.text !== next) {
                    editor.syncing = true; editor.text = next; editor.syncing = false
                }
            }
            function onEditedTextChanged() { refreshEditor() }
            function onOriginalTextChanged() { refreshEditor() }
            function onShowingOriginalChanged() { refreshEditor() }
        }
        Shortcut { sequence: "Ctrl+Return"; enabled: !bridge.settingsVisible && bridge.phase === 0 && editor.text.trim().length > 0; onActivated: bridge.copyAndClose() }
        Shortcut { sequence: "Escape"; onActivated: bridge.hide() }
        Shortcut { sequence: "Ctrl+M"; enabled: !bridge.settingsVisible; onActivated: bridge.showCompact() }
        header: Item {
            implicitHeight: 76
            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.LeftButton
                cursorShape: pressed ? Qt.ClosedHandCursor : Qt.OpenHandCursor
                onPressed: win.startSystemMove()
            }
            RowLayout {
                anchors.fill: parent; anchors.leftMargin: 28; anchors.rightMargin: 24
                spacing: 12
                WaveMark { Layout.alignment: Qt.AlignVCenter }
                Text { text: "Dictate"; font.pixelSize: 19; font.weight: Font.DemiBold; color: app.ink; Layout.fillWidth: true }
                Action {
                    glyph: bridge.settingsVisible ? "back" : "settings"
                    text: bridge.settingsVisible ? qsTr("Back") : qsTr("Settings")
                    quiet: true
                    onClicked: { if (bridge.settingsVisible) bridge.backFromSettings(); else bridge.openSettings() }
                }
                Action {
                    glyph: "compact"; quiet: true
                    visible: !bridge.settingsVisible
                    Accessible.name: qsTr("Compact bar (Ctrl+M)")
                    onClicked: bridge.showCompact()
                }
                Action {
                    glyph: "history"; quiet: true
                    visible: !bridge.settingsVisible && JSON.parse(bridge.historyJson).length > 1
                    Accessible.name: qsTr("Show history")
                    onClicked: win.historyOpen = !win.historyOpen
                }
                Action {
                    glyph: "close"; quiet: true
                    Accessible.name: qsTr("Close Dictate")
                    onClicked: bridge.teardown()
                }
            }
        }
        StackLayout {
            anchors.fill: parent
            currentIndex: bridge.settingsVisible ? 1 : 0
            Item {
              ColumnLayout {
                anchors.fill: parent
                anchors.leftMargin: 28; anchors.rightMargin: 28; anchors.bottomMargin: 24
                spacing: 16
                Choice {
                    Layout.fillWidth: true
                    model: JSON.parse(bridge.historyJson)
                    currentIndex: bridge.historyIndex
                    visible: win.historyOpen && count > 1
                    enabled: bridge.phase === 0
                    onActivated: bridge.selectSession(currentIndex)
                    Accessible.name: qsTr("Recent dictations")
                }
                Rectangle {
                    visible: bridge.errorText.length > 0
                    Layout.fillWidth: true
                    implicitHeight: errorContent.implicitHeight + 28
                    radius: 10; color: app.dangerSoft
                    ColumnLayout {
                        id: errorContent
                        anchors.fill: parent; anchors.margins: 14; spacing: 8
                        RowLayout {
                            Layout.fillWidth: true
                            Text { text: bridge.errorText; color: app.danger; font.pixelSize: 13; wrapMode: Text.WordWrap; Layout.fillWidth: true; Accessible.role: Accessible.AlertMessage }
                            Action { glyph: "close"; quiet: true; Accessible.name: qsTr("Dismiss notification"); onClicked: bridge.dismissError() }
                        }
                        Flow {
                            visible: !bridge.pasteError
                            Layout.fillWidth: true; spacing: 8
                            Action { text: qsTr("Retry"); quiet: true; visible: !bridge.pasteError; enabled: bridge.canRetry && bridge.phase === 0; onClicked: bridge.retry() }
                            Action { text: qsTr("Open settings"); quiet: true; onClicked: bridge.openSettings() }
                        }
                    }
                }
                ColumnLayout {
                    visible: win.empty
                    spacing: 16
                    Layout.fillWidth: true; Layout.fillHeight: true
                    Item { Layout.fillHeight: true }
                    Rectangle {
                        width: 60; height: 60; radius: 18; color: app.accentSoft
                        Glyph { name: "mic"; tint: app.accent; width: 28; height: 28; anchors.centerIn: parent }
                    }
                    Text { text: qsTr("Ready when you are."); color: app.ink; font.pixelSize: win.width < 480 ? 27 : 32; font.weight: Font.DemiBold; font.letterSpacing: -0.7; Layout.fillWidth: true }
                    Text { text: qsTr("Start a recording, then review and copy your text."); color: app.muted; font.pixelSize: 14; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                    Action { text: qsTr("Start recording"); glyph: "mic"; primary: true; Layout.topMargin: 8; onClicked: bridge.toggle() }
                    Item { Layout.fillHeight: true }
                    Caption { text: qsTr("Your drafts stay here until you close the app."); Layout.fillWidth: true }
                }
                ColumnLayout {
                    visible: !win.empty
                    Layout.fillWidth: true; Layout.fillHeight: true; spacing: 12
                    RowLayout {
                        Layout.fillWidth: true; spacing: 6
                        Action {
                            text: qsTr("Edited"); quiet: bridge.showingOriginal
                            enabled: bridge.originalText.length > 0
                            onClicked: if (bridge.showingOriginal) bridge.toggleOriginal()
                        }
                        Action {
                            text: qsTr("Original"); quiet: !bridge.showingOriginal
                            enabled: bridge.originalText.length > 0
                            onClicked: if (!bridge.showingOriginal) bridge.toggleOriginal()
                        }
                        Item { Layout.fillWidth: true }
                        Caption { text: bridge.showingOriginal ? qsTr("Read-only") : bridge.phase > 1 ? qsTr("Processing…") : qsTr("Editable") }
                    }
                    Rectangle {
                        Layout.fillWidth: true; Layout.fillHeight: true
                        color: app.surface; radius: 12; border.color: editor.activeFocus ? app.accent : app.line
                        ScrollView {
                    ScrollBar.vertical: ScrollRail {}
                    ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
                            id: textScroll
                            anchors.fill: parent; anchors.margins: 1
                            clip: true
                            TextArea {
                                id: editor
                                property bool syncing: false
                                readOnly: bridge.showingOriginal || bridge.historyJson === "[]"
                                wrapMode: TextArea.Wrap
                                padding: 20
                                selectByMouse: true
                                color: app.ink; placeholderTextColor: app.muted
                                selectionColor: app.accent; selectedTextColor: app.accentInk
                                font.pixelSize: 16
                                background: null
                                placeholderText: bridge.phase > 1 ? qsTr("Your words are on their way…") : qsTr("Dictation text")
                                onTextChanged: if (!syncing && !bridge.showingOriginal) bridge.editDraft(text)
                                Accessible.name: bridge.showingOriginal ? qsTr("Original transcript") : qsTr("Dictation text")
                            }
                        }
                    }
                    RowLayout {
                        Layout.fillWidth: true; spacing: 8
                        Rectangle { width: 6; height: 6; radius: 3; color: bridge.phase > 1 ? app.accent : app.muted }
                        Caption { text: bridge.statusText; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                        Action { text: qsTr("Retry editing"); quiet: true; visible: bridge.phase === 0 && bridge.canRetry && bridge.errorText.length === 0; onClicked: bridge.retry() }
                        Action { text: qsTr("Cancel"); quiet: true; visible: bridge.phase > 1; onClicked: bridge.cancel() }
                    }
                    RowLayout {
                        Layout.fillWidth: true; spacing: 8
                        Action { text: qsTr("Copy text"); glyph: "copy"; primary: true; enabled: editor.text.trim().length > 0; onClicked: bridge.copyCurrent() }
                        Action { text: bridge.phase === 1 ? qsTr("Finish") : qsTr("New recording"); glyph: "mic"; enabled: bridge.phase <= 1; onClicked: bridge.toggle() }
                        Item { Layout.fillWidth: true }
                        Caption { text: "Ctrl ↵"; visible: win.width > 500 }
                    }
                }
            }
            }
            ColumnLayout {
                spacing: 0
                ScrollView {
                    ScrollBar.vertical: ScrollRail {}
                    ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
                    Layout.fillWidth: true; Layout.fillHeight: true
                    clip: true; contentWidth: availableWidth
                    ColumnLayout {
                        width: parent.width; spacing: 12
                        Item { implicitHeight: 0 }
                        ColumnLayout {
                            Layout.fillWidth: true; Layout.leftMargin: 28; Layout.rightMargin: 28
                            spacing: 10
                            Text { text: qsTr("Settings"); color: app.ink; font.pixelSize: 28; font.weight: Font.DemiBold; font.letterSpacing: -0.5 }
                            Caption { text: qsTr("Choose how your voice becomes text."); Layout.fillWidth: true }
                            Section { title: qsTr("Workflow"); description: qsTr("Set the defaults for your recording shortcut.") }
                            FieldLabel { text: qsTr("Result view") }
                            Choice {
                                Layout.fillWidth: true
                                model: [qsTr("Compact bottom bar"), qsTr("Full editor")]
                                currentIndex: bridge.cfgPresentation
                                onActivated: bridge.cfgPresentation = currentIndex
                                Accessible.name: qsTr("Result view")
                            }
                            FieldLabel { text: qsTr("After processing") }
                            Choice {
                                Layout.fillWidth: true
                                model: [qsTr("Review, then copy & close"), qsTr("Copy & close automatically"), qsTr("Paste & close automatically")]
                                currentIndex: bridge.cfgOutput
                                onActivated: bridge.cfgOutput = currentIndex
                                Accessible.name: qsTr("After processing")
                            }
                            Caption {
                                text: bridge.cfgOutput === 2 ? qsTr("Automatic paste requires desktop portal support. The text is also copied to the clipboard.") : bridge.cfgOutput === 1 ? qsTr("The final edited text is copied and the interface closes. Errors stay visible for review.") : qsTr("Review the beginning of the result in the bar, or expand it to edit the full text.")
                                Layout.fillWidth: true
                            }
                            Rule { Layout.topMargin: 12 }
                            Section { title: qsTr("Transcription"); description: qsTr("The service that turns your recording into words.") }
                            Choice {
                                Layout.fillWidth: true
                                model: [qsTr("ChatGPT proxy"), qsTr("ElevenLabs · Scribe v2")]
                                currentIndex: bridge.cfgSttProvider
                                onActivated: bridge.cfgSttProvider = currentIndex
                                Accessible.name: qsTr("Transcription provider")
                            }
                            FieldLabel { visible: bridge.cfgSttProvider === 1; text: qsTr("ElevenLabs API key") }
                            Field { visible: bridge.cfgSttProvider === 1; Layout.fillWidth: true; echoMode: TextInput.Password; text: bridge.cfgElevenlabsApiKey; onTextEdited: bridge.cfgElevenlabsApiKey = text; Accessible.name: qsTr("ElevenLabs API key") }
                            Caption { visible: bridge.cfgSttProvider === 1; text: qsTr("You can also provide the key through ELEVENLABS_API_KEY."); Layout.fillWidth: true }
                            Rule { Layout.topMargin: 12 }
                            Section { title: qsTr("Text editing"); description: bridge.cfgSttProvider === 0 ? qsTr("This service handles both transcription and editing.") : qsTr("Refine punctuation and apply your spoken corrections.") }
                            FieldLabel { text: qsTr("Service URL") }
                            Field { Layout.fillWidth: true; text: bridge.cfgBaseUrl; placeholderText: "https://proxy.example.com/v1"; onTextEdited: bridge.cfgBaseUrl = text; Accessible.name: qsTr("Service URL") }
                            FieldLabel { text: qsTr("API key") }
                            Field { Layout.fillWidth: true; echoMode: TextInput.Password; text: bridge.cfgApiKey; onTextEdited: bridge.cfgApiKey = text; Accessible.name: qsTr("API key") }
                            FieldLabel { text: qsTr("Editing model") }
                            Field { Layout.fillWidth: true; text: bridge.cfgModel; placeholderText: qsTr("Model name"); onTextEdited: bridge.cfgModel = text; Accessible.name: qsTr("Editing model") }
                            Caption { text: qsTr("Keys entered here are saved on this device. Environment keys take precedence."); Layout.fillWidth: true }
                            Rule { Layout.topMargin: 12 }
                            Section { title: qsTr("Recording"); description: qsTr("Choose your language and microphone.") }
                            FieldLabel { text: qsTr("Language") }
                            Choice {
                                Layout.fillWidth: true
                                model: app.languageOptions; textRole: "label"; valueRole: "code"
                                editable: true
                                currentIndex: app.languageOptions.findIndex(item => item.code === bridge.cfgLanguage)
                                onActivated: bridge.cfgLanguage = currentValue
                                onAccepted: {
                                    const value = editText.trim()
                                    const language = app.languages.find(item => item.label.toLowerCase() === value.toLowerCase() || item.code === value.toLowerCase())
                                    bridge.cfgLanguage = language ? language.code : value
                                }
                                Accessible.name: qsTr("Language")
                            }
                            FieldLabel { text: qsTr("Microphone") }
                            RowLayout {
                                Layout.fillWidth: true; spacing: 8
                                Choice {
                                    Layout.fillWidth: true
                                    model: app.microphoneOptions; textRole: "label"; valueRole: "name"
                                    currentIndex: app.microphoneOptions.findIndex(item => item.name === bridge.cfgMicrophone)
                                    onActivated: bridge.cfgMicrophone = currentValue
                                    Accessible.name: qsTr("Microphone")
                                }
                                Action { text: qsTr("Refresh"); onClicked: bridge.refreshMicrophones() }
                            }
                            Caption { text: app.microphoneInfo.error; visible: text.length > 0; color: app.danger; Layout.fillWidth: true }
                            Rule { Layout.topMargin: 12 }
                            Section { title: qsTr("Automatic paste"); description: qsTr("Insert the result into the focused application.") }
                            Caption {
                                text: bridge.pasteStatus.length > 0 ? bridge.pasteStatus : bridge.pasteReady ? qsTr("Connected and ready to paste.") : bridge.pasteSaved ? qsTr("Permission saved. Reconnect to check access.") : qsTr("Allow access through your desktop's permission dialog.")
                                Layout.fillWidth: true
                            }
                            Action { text: bridge.pasteConnecting ? qsTr("Connecting…") : bridge.pasteReady ? qsTr("Connected") : bridge.pasteSaved ? qsTr("Check connection") : qsTr("Connect automatic paste"); glyph: bridge.pasteReady ? "check" : ""; enabled: !bridge.pasteReady && !bridge.pasteConnecting; onClicked: bridge.connectPaste() }
                            Basic.CheckBox {
                                id: terminalCheck
                                text: qsTr("Use Ctrl+Shift+V for terminals")
                                checked: bridge.cfgPasteShift; onToggled: bridge.cfgPasteShift = checked
                                spacing: 10
                                indicator: Rectangle {
                                    implicitWidth: 20; implicitHeight: 20; y: (terminalCheck.height - height) / 2
                                    radius: 5; color: terminalCheck.checked ? app.accent : app.surface
                                    border.color: terminalCheck.visualFocus ? app.accent : app.line
                                    Glyph { anchors.centerIn: parent; width: 15; height: 15; name: "check"; tint: app.accentInk; visible: terminalCheck.checked }
                                }
                                contentItem: Text { text: terminalCheck.text; color: app.ink; font.pixelSize: 13; leftPadding: terminalCheck.indicator.width + terminalCheck.spacing; verticalAlignment: Text.AlignVCenter }
                            }
                            Item { implicitHeight: 16 }
                        }
                    }
                }
                Rule {}
                RowLayout {
                    Layout.fillWidth: true; Layout.margins: 20; spacing: 12
                    Caption { text: bridge.settingsStatus; Layout.fillWidth: true }
                    Action { text: qsTr("Resume"); visible: bridge.canRetry; enabled: bridge.phase === 0; onClicked: bridge.retry() }
                    Action { text: qsTr("Save settings"); primary: true; enabled: bridge.phase === 0; onClicked: bridge.saveSettings() }
                }
            }
        }
    }
    property Window osd: Window {
        id: osdWin
        title: "Dictate bar"
        width: Math.min(620, screen.width - 32)
        height: osdContent.implicitHeight + 36
        visible: bridge.osdVisible
        color: "transparent"
        flags: Qt.FramelessWindowHint
        LayerShell.Window.anchors: LayerShell.Window.AnchorBottom
        LayerShell.Window.margins: ({ top: 0, left: 0, right: 0, bottom: 16 })
        LayerShell.Window.layer: LayerShell.Window.LayerOverlay
        LayerShell.Window.keyboardInteractivity: LayerShell.Window.KeyboardInteractivityNone
        LayerShell.Window.exclusionZone: 0
        LayerShell.Window.wantsToBeOnActiveScreen: true
        LayerShell.Window.scope: "dictate"
        Rectangle {
            anchors.fill: parent; anchors.margins: 6
            color: app.surface; radius: 16; border.color: app.line
            ColumnLayout {
                id: osdContent
                anchors.fill: parent; anchors.margins: 12; spacing: 10
                RowLayout {
                    spacing: 12
                    Rectangle {
                        width: 34; height: 34; radius: 11
                        color: bridge.phase === 1 ? app.dangerSoft : app.accentSoft
                        Glyph {
                            anchors.centerIn: parent
                            name: bridge.phase === 1 || !bridge.hasResult ? "mic" : "check"
                            tint: bridge.phase === 1 ? app.danger : app.accent
                        }
                    }
                    ColumnLayout {
                        spacing: 4; Layout.fillWidth: true
                        RowLayout {
                            Layout.fillWidth: true; spacing: 10
                            Text {
                                text: bridge.phase === 1 ? qsTr("Recording") : bridge.errorText.length ? qsTr("Needs attention") : bridge.phase > 1 ? bridge.statusText : bridge.hasResult ? qsTr("Ready to copy") : qsTr("Ready to record")
                                color: app.ink; font.pixelSize: 13; font.weight: Font.Medium
                                elide: Text.ElideRight; Layout.fillWidth: true
                            }
                            Caption { visible: bridge.phase === 1; text: "%1:%2".arg(Math.floor(bridge.seconds / 60)).arg(("0" + bridge.seconds % 60).slice(-2)) }
                        }
                        Text {
                            visible: bridge.hasResult && bridge.phase !== 1
                            text: (bridge.showingOriginal ? bridge.originalText : bridge.editedText).replace(/\s+/g, " ")
                            color: app.muted; font.pixelSize: 13
                            wrapMode: Text.WordWrap; maximumLineCount: 2; elide: Text.ElideRight
                            Layout.fillWidth: true
                        }
                    }
                    Action { text: qsTr("Finish"); glyph: "stop"; primary: true; visible: bridge.phase === 1; onClicked: bridge.toggle() }
                    Action { text: qsTr("Record"); glyph: "mic"; primary: true; visible: bridge.phase === 0 && !bridge.hasResult && bridge.errorText.length === 0; onClicked: bridge.toggle() }
                    Action { text: qsTr("Copy & close"); glyph: "copy"; primary: true; visible: bridge.phase === 0 && bridge.hasResult; onClicked: bridge.copyAndClose() }
                    Action { glyph: "expand"; quiet: true; Accessible.name: qsTr("Open full editor"); onClicked: bridge.expand() }
                    Action { glyph: "close"; quiet: true; Accessible.name: bridge.phase === 0 ? qsTr("Hide bar") : qsTr("Cancel"); onClicked: bridge.hide() }
                }
                Row {
                    visible: bridge.phase === 1
                    spacing: 3; Layout.fillWidth: true
                    Repeater {
                        model: 48
                        Rectangle {
                            required property int index
                            width: Math.max(1, (osdContent.width - 47 * 3) / 48); height: 3; radius: 1.5
                            color: index / 48 < bridge.level ? app.accent : app.line
                        }
                    }
                }
                Text {
                    visible: bridge.errorText.length > 0
                    text: bridge.errorText; color: app.danger; font.pixelSize: 12
                    wrapMode: Text.WordWrap; maximumLineCount: 3; elide: Text.ElideRight
                    Layout.fillWidth: true
                }
                RowLayout {
                    visible: bridge.errorText.length > 0 && bridge.phase === 0
                    Layout.fillWidth: true; spacing: 8
                    Action { text: qsTr("Retry"); quiet: true; visible: bridge.canRetry && !bridge.pasteError; onClicked: bridge.retry() }
                    Action { text: qsTr("Settings"); glyph: "settings"; quiet: true; visible: !bridge.pasteError; onClicked: bridge.openSettings() }
                    Item { Layout.fillWidth: true }
                }
            }
        }
    }
    Component.onCompleted: {
        Qt.application.domain = "okhsunrog.ru"
        bridge.initialize()
    }
}
