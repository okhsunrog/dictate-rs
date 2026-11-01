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
        title: bridge.settingsVisible ? qsTr("Настройки диктовки") : qsTr("Диктовка")
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
                Label { text: bridge.settingsVisible ? qsTr("Настройки") : qsTr("Диктовка"); font.bold: true; font.pointSize: 14; Layout.fillWidth: true }
                ToolButton {
                    text: bridge.settingsVisible ? qsTr("К тексту") : qsTr("Настройки")
                    icon.name: bridge.settingsVisible ? "go-previous" : "configure"
                    onClicked: bridge.settingsVisible = !bridge.settingsVisible
                }
                ToolButton {
                    text: qsTr("Ещё"); icon.name: "application-menu"
                    onClicked: appMenu.popup()
                    Menu {
                        id: appMenu
                        MenuItem { text: qsTr("История"); enabled: JSON.parse(bridge.historyJson).length > 1; checkable: true; checked: win.historyOpen; onTriggered: win.historyOpen = !win.historyOpen }
                        MenuItem { text: qsTr("Исходная расшифровка"); enabled: bridge.originalText.length > 0; checkable: true; checked: bridge.showingOriginal; onTriggered: bridge.toggleOriginal() }
                        MenuItem { text: bridge.originalText.length ? qsTr("Повторить редактирование") : qsTr("Повторить распознавание"); enabled: bridge.canRetry && bridge.phase === 0; onTriggered: bridge.retry() }
                        MenuSeparator {}
                        MenuItem { text: qsTr("Выйти из приложения"); onTriggered: bridge.teardown() }
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
                    Accessible.name: qsTr("Последние диктовки, только в памяти")
                }
                RowLayout {
                    visible: !win.empty
                    Layout.fillWidth: true
                    BusyIndicator { running: bridge.phase > 1; visible: running; implicitWidth: 24; implicitHeight: 24 }
                    Label { text: bridge.statusText; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                    ToolButton { text: qsTr("Остановить"); visible: bridge.phase > 1; onClicked: bridge.cancel() }
                }
                Frame {
                    visible: bridge.errorText.length > 0
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        Label { text: bridge.errorText; wrapMode: Text.WordWrap; Layout.fillWidth: true; Accessible.role: Accessible.AlertMessage }
                        Flow {
                            Layout.fillWidth: true; spacing: 8
                            Button { text: qsTr("Повторить"); visible: !bridge.pasteError; enabled: bridge.canRetry && bridge.phase === 0; onClicked: bridge.retry() }
                            Button { text: qsTr("Настроить подключение"); onClicked: bridge.settingsVisible = true }
                        }
                    }
                }
                ColumnLayout {
                    visible: win.empty
                    Layout.fillWidth: true; Layout.fillHeight: true
                    spacing: 16
                    Item { Layout.fillHeight: true }
                    Label { text: qsTr("Преврати речь в текст"); font.pointSize: 18; font.bold: true; Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter }
                    Label { text: qsTr("Запиши, проверь и скопируй."); Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter }
                    Button { text: qsTr("Начать запись"); highlighted: true; Layout.alignment: Qt.AlignHCenter; onClicked: bridge.toggle() }
                    Item { Layout.fillHeight: true }
                }
                Label { visible: bridge.showingOriginal; text: qsTr("Исходная расшифровка · только чтение") }
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
                        placeholderText: bridge.phase > 1 ? qsTr("Распознаём запись…") : qsTr("Текст диктовки")
                        onTextChanged: if (!syncing && !bridge.showingOriginal) bridge.editDraft(text)
                        Accessible.name: bridge.showingOriginal ? qsTr("Исходная расшифровка") : qsTr("Текст диктовки")
                    }
                }
                RowLayout {
                    visible: !win.empty
                    Layout.fillWidth: true; spacing: 8
                    Button { text: qsTr("Копировать"); highlighted: true; enabled: editor.text.trim().length > 0; onClicked: bridge.copyCurrent() }
                    Button { text: bridge.phase === 1 ? qsTr("Завершить") : qsTr("Новая запись"); enabled: bridge.phase <= 1; onClicked: bridge.toggle() }
                    Item { Layout.fillWidth: true }
                }
            }
            ScrollView {
                clip: true
                contentWidth: availableWidth
                ColumnLayout {
                    width: parent.width; spacing: 10
                    Label { text: qsTr("Распознавание"); font.bold: true }
                    Label { text: qsTr("Адрес сервиса (с /v1)") }
                    TextField { Layout.fillWidth: true; text: bridge.cfgBaseUrl; onTextEdited: bridge.cfgBaseUrl = text; Accessible.name: qsTr("Адрес сервиса") }
                    Label { text: qsTr("Ключ API") }
                    TextField { Layout.fillWidth: true; echoMode: TextInput.Password; text: bridge.cfgApiKey; onTextEdited: bridge.cfgApiKey = text; Accessible.name: qsTr("Ключ API") }
                    Label { text: qsTr("Ключ из DICTATE_API_KEY используется без сохранения в файл. Введённый здесь ключ хранится локально с правами 0600."); wrapMode: Text.WordWrap; Layout.fillWidth: true; opacity: 0.7 }
                    Label { text: qsTr("Автовставка"); font.bold: true }
                    Label { text: qsTr("Текст вставляется в активное на момент готовности приложение."); wrapMode: Text.WordWrap; Layout.fillWidth: true }
                    CheckBox { text: qsTr("Совместимость с терминалами"); checked: bridge.cfgPasteShift; onToggled: bridge.cfgPasteShift = checked }
                    Label { text: bridge.pasteError ? qsTr("Не удалось подключить автовставку. Подробности — ниже.") : bridge.pasteReady ? (bridge.pasteSaved ? qsTr("Автовставка подключена. Разрешение сохранено.") : qsTr("Подключено на этот запуск. Портал не разрешил сохранить доступ.")) : bridge.pasteSaved ? qsTr("Разрешение сохранено. Подключение восстановится автоматически при вставке.") : qsTr("Разреши автовставку один раз для следующих запусков."); wrapMode: Text.WordWrap; Layout.fillWidth: true }
                    Button { text: bridge.pasteReady ? qsTr("Автовставка подключена") : bridge.pasteSaved ? qsTr("Проверить подключение") : qsTr("Подключить автовставку"); enabled: !bridge.pasteReady; onClicked: bridge.connectPaste() }
                    Label { text: qsTr("Дополнительные настройки"); font.bold: true }
                    Label { text: qsTr("Язык (пусто — определить автоматически)") }
                    TextField { Layout.fillWidth: true; text: bridge.cfgLanguage; placeholderText: "ru, en"; onTextEdited: bridge.cfgLanguage = text }
                    Label { text: qsTr("Модель редактирования") }
                    TextField { Layout.fillWidth: true; text: bridge.cfgModel; onTextEdited: bridge.cfgModel = text }
                    Label { text: qsTr("Микрофон PipeWire (пусто — системный)") }
                    TextField { Layout.fillWidth: true; text: bridge.cfgMicrophone; onTextEdited: bridge.cfgMicrophone = text }
                    Button { text: qsTr("Сохранить настройки"); enabled: bridge.phase === 0; onClicked: bridge.saveSettings() }
                    Label { text: bridge.settingsStatus; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                    Button { text: qsTr("Продолжить обработку записи"); visible: bridge.canRetry; enabled: bridge.phase === 0; onClicked: bridge.retry() }
                    Label { text: qsTr("Последние 10 текстов хранятся до выхода из приложения. Аудио для повтора — до следующей записи."); wrapMode: Text.WordWrap; Layout.fillWidth: true; opacity: 0.7 }
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
                    text: bridge.phase === 1 ? qsTr("Запись · %1:%2").arg(Math.floor(bridge.seconds / 60)).arg(("0" + bridge.seconds % 60).slice(-2)) : bridge.errorText.length ? qsTr("Не удалось начать запись") : bridge.statusText
                    Layout.fillWidth: true; wrapMode: Text.WordWrap
                }
                Button { text: qsTr("Завершить"); visible: bridge.phase === 1; onClicked: bridge.toggle() }
                ToolButton { text: bridge.phase === 0 ? qsTr("Закрыть") : qsTr("Отмена"); onClicked: { if (bridge.phase === 0) bridge.osdVisible = false; else bridge.cancel() } }
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
