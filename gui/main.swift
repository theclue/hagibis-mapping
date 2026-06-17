import AppKit
import Foundation

@_silgen_name("hagibis_start") func hagibis_start() -> Int32
@_silgen_name("hagibis_stop") func hagibis_stop()
@_silgen_name("hagibis_is_running") func hagibis_is_running() -> Int32
@_silgen_name("hagibis_status_json") func hagibis_status_json(_ buf: UnsafeMutablePointer<CChar>, _ size: Int32) -> Int32
@_silgen_name("hagibis_config_json") func hagibis_config_json(_ buf: UnsafeMutablePointer<CChar>, _ size: Int32) -> Int32
@_silgen_name("hagibis_save_config_json") func hagibis_save_config_json(_ json: UnsafePointer<CChar>) -> Int32
@_silgen_name("hagibis_reload_config") func hagibis_reload_config() -> Int32
@_silgen_name("hagibis_elevate") func hagibis_elevate(_ path: UnsafePointer<CChar>) -> Int32
@_silgen_name("hagibis_log") func hagibis_log(_ level: Int32, _ cat: UnsafePointer<CChar>, _ msg: UnsafePointer<CChar>)

if geteuid() != 0 {
    if let execPath = Bundle.main.executablePath {
        _ = execPath.withCString { hagibis_elevate($0) }
    }
    exit(0)
}

// ── Key capture text field ──────────────────────────────────────────────

class KeyCaptureField: NSTextField {
    var onKeyCombo: ((String) -> Void)?

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        interpret(event); return true
    }
    override func keyDown(with event: NSEvent) { interpret(event) }
    override func flagsChanged(with event: NSEvent) {}

    private func interpret(_ event: NSEvent) {
        var parts: [String] = []
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        if flags.contains(.control) { parts.append("Ctrl") }
        if flags.contains(.shift) { parts.append("Shift") }
        if flags.contains(.option) { parts.append("Alt") }
        if flags.contains(.command) { parts.append("Cmd") }
        let key = event.charactersIgnoringModifiers?.uppercased() ?? ""
        let mapped: String? = switch event.keyCode {
            case 36: "Enter"; case 48: "Tab"; case 49: "Space"; case 51: "Backspace"
            case 53: "Escape"; case 117: "Delete"
            case 115: "Home"; case 119: "End"; case 116: "PageUp"; case 121: "PageDown"
            case 122: "F1"; case 120: "F2"; case 99: "F3"; case 118: "F4"
            case 96: "F5"; case 97: "F6"; case 98: "F7"; case 100: "F8"
            case 101: "F9"; case 109: "F10"; case 103: "F11"; case 111: "F12"
            case 105: "F13"; case 107: "F14"; case 113: "F15"
            case 126: "ArrowUp"; case 125: "ArrowDown"
            case 123: "ArrowLeft"; case 124: "ArrowRight"
            default: nil
        }
        if let m = mapped, !m.isEmpty { parts.append(m) }
        else if key.count == 1 { parts.append(key) }
        else { return }
        let combo = parts.joined(separator: "+")
        stringValue = combo
        onKeyCombo?(combo)
    }
}

// ── App Delegate ────────────────────────────────────────────────────────

class AppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate {
    var statusItem: NSStatusItem!
    var timer: Timer?
    var window: NSWindow?
    var editor: NSWindow?

    var statusField: NSTextField!, appField: NSTextField!
    var btnTL: NSTextField!, btnTLh: NSTextField!, btnBR: NSTextField!, btnBRh: NSTextField!
    var knobCW: NSTextField!, knobCCW: NSTextField!, knobClick: NSTextField!
    var playField: NSTextField!

    // Editor controls (single-event only)
    var edAppDropdown: NSPopUpButton!
    var edKey: String = ""
    var edConfig: [String: Any] = [:]
    var edRadioKey: NSButton!, edRadioMedia: NSButton!, edRadioMouse: NSButton!
    var edKeyField: KeyCaptureField!
    var edMediaDropdown: NSPopUpButton!
    var edMouseDropdown: NSPopUpButton!
    var edLabelField: NSTextField!
    var edCurType: String = "Keyboard"

    // Defaults shown when stopped
    let devTL = "Cmd+R", devTLh = "Ctrl+Q", devBR = "Cmd+F13", devBRh = "Ctrl+3"
    let devCW = "Vol+", devCCW = "Vol−", devClick = "Mute", devPlay = "Play/Pause"
    let dispW: CGFloat = 300
    var focusedAppId: String = ""

// ── App lifecycle ──────────────────────────────────────────────────────

    func applicationDidFinishLaunching(_ n: Notification) {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        if let b = statusItem.button { b.title = "HM"; b.font = .monospacedDigitSystemFont(ofSize: 12, weight: .bold) }
        let m = NSMenu()
        let toggleMenuItem = NSMenuItem(title: "Stop", action: #selector(toggleEngine), keyEquivalent: "")
        m.addItem(toggleMenuItem); m.addItem(.separator())
        m.addItem(NSMenuItem(title: "Show Monitor", action: #selector(showWindow), keyEquivalent: ""))
        m.addItem(.separator())
        m.addItem(NSMenuItem(title: "About Hagibis Mapping", action: #selector(showAbout), keyEquivalent: ""))
        m.addItem(NSMenuItem(title: "Quit", action: #selector(quitApp), keyEquivalent: "q"))
        statusItem.menu = m

        startEngine()
        timer = Timer.scheduledTimer(withTimeInterval: 0.15, repeats: true) { [weak self] _ in
            self?.updateStatus()
        }
    }

    @objc func startEngine() {
        if hagibis_start() != 0 { NSLog("[Hagibis Mapping] hagibis_start() failed") }
        updateStatus()
    }
    @objc func stopEngine() { hagibis_stop(); updateStatus() }
    @objc func toggleEngine() { if hagibis_is_running() != 0 { stopEngine() } else { startEngine() } }

// ── Status polling ────────────────────────────────────────────────────

    func updateStatus() {
        var buf = [CChar](repeating: 0, count: 8192)
        let len = buf.withUnsafeMutableBufferPointer { ptr -> Int32 in
            guard let addr = ptr.baseAddress else { return 0 }
            return hagibis_status_json(addr, 8192)
        }
        guard len > 0,
              let json = String(bytes: Data(bytes: buf, count: Int(len)), encoding: .utf8),
              let data = json.data(using: .utf8),
              let s = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return }

        let running = s["running"] as? Bool ?? false
        let app     = s["focused_app_name"] as? String ?? "—"
        let appId   = s["focused_app_id"] as? String ?? ""
        let cons    = UInt8(truncatingIfNeeded: (s["consumer_sticky"] as? Int) ?? (s["consumer"] as? Int) ?? 0)
        let keys: [UInt8] = (s["keyboard_keys"] as? [Int])?.map { UInt8(truncatingIfNeeded: $0) } ?? []
        let lblTL   = s["btn_tl"] as? String ?? "—", lblTLh = s["btn_tl_hold"] as? String ?? "—"
        let lblBR   = s["btn_br"] as? String ?? "—", lblBRh = s["btn_br_hold"] as? String ?? "—"
        let lblCW   = s["knob_cw"] as? String ?? "—", lblCCW = s["knob_ccw"] as? String ?? "—"
        let lblClick = s["knob_click"] as? String ?? "—", lblPlay = s["play_pause"] as? String ?? "—"
        let enginePresent = s["engine_present"] as? Bool ?? false

        let isOn = { (code: UInt8) -> Bool in keys.contains(code) }
        let d = { (on: Bool) -> String in on ? "●" : "○" }

        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.focusedAppId = appId
            self.statusItem?.button?.title = running ? "●HM" : "○HM"
            if let tmi = self.statusItem?.menu?.item(at: 0) {
                tmi.title = enginePresent ? "Stop" : "Start"
            }
            self.statusField?.stringValue = running ? "● Running" : "○ Stopped"
            self.appField?.stringValue = app

            let tl = running ? lblTL : self.devTL, tl_h = running ? lblTLh : self.devTLh
            let br = running ? lblBR : self.devBR, br_h = running ? lblBRh : self.devBRh
            let cw = running ? lblCW : self.devCW, ccw = running ? lblCCW : self.devCCW
            let click = running ? lblClick : self.devClick, play = running ? lblPlay : self.devPlay

            self.btnTL?.stringValue = "\(d(isOn(0x0F))) \(tl)"
            self.btnTL?.textColor = isOn(0x0F) ? .systemGreen : NSColor(white: 0.15, alpha: 1)
            self.btnTLh?.stringValue = "\(d(isOn(0x14))) \(tl_h)"
            self.btnTLh?.textColor = isOn(0x14) ? .systemGreen : NSColor(white: 0.15, alpha: 1)
            self.btnBR?.stringValue = "\(d(isOn(0x46))) \(br)"
            self.btnBR?.textColor = isOn(0x46) ? .systemGreen : NSColor(white: 0.15, alpha: 1)
            self.btnBRh?.stringValue = "\(d(isOn(0x20))) \(br_h)"
            self.btnBRh?.textColor = isOn(0x20) ? .systemGreen : NSColor(white: 0.15, alpha: 1)
            self.knobCW?.stringValue = "\(d(cons & 0x01 != 0)) \(cw)"
            self.knobCW?.textColor = cons & 0x01 != 0 ? .systemGreen : NSColor(white: 0.15, alpha: 1)
            self.knobCCW?.stringValue = "\(d(cons & 0x02 != 0)) \(ccw)"
            self.knobCCW?.textColor = cons & 0x02 != 0 ? .systemGreen : NSColor(white: 0.15, alpha: 1)
            self.knobClick?.stringValue = "\(d(cons & 0x04 != 0)) \(click)"
            self.knobClick?.textColor = cons & 0x04 != 0 ? .systemGreen : NSColor(white: 0.15, alpha: 1)
            self.playField?.stringValue = "\(d(cons & 0x40 != 0)) \(play)"
            self.playField?.textColor = cons & 0x40 != 0 ? .systemGreen : NSColor(white: 0.15, alpha: 1)
        }
    }

// ── Monitor window ───────────────────────────────────────────────────

    @objc func showWindow() {
        if window == nil { createWindow() }
        NSApp.setActivationPolicy(.regular)
        window?.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    func makePill(_ xf: Double, _ yf: Double, _ ra: Bool, _ w: CGFloat, _ tag: String,
                  _ iv: NSImageView, _ imgDH: CGFloat, _ s: CGFloat) -> NSTextField {
        let pw: CGFloat = 65 * s
        let ox: CGFloat = ra ? CGFloat(1 - 0.10) * w - pw : CGFloat(xf) * w
        let oy = imgDH - CGFloat(yf) * imgDH - 14
        let bg = NSView(frame: NSRect(x: ox - 4, y: oy - 1, width: pw + 8, height: 18))
        bg.wantsLayer = true; bg.layer?.backgroundColor = NSColor(white: 1, alpha: 0.8).cgColor
        bg.layer?.cornerRadius = 4
        bg.toolTip = tag
        iv.addSubview(bg)
        let f = NSTextField(labelWithString: "○ —")
        f.font = .monospacedDigitSystemFont(ofSize: 10, weight: .regular)
        f.textColor = NSColor(white: 0.15, alpha: 1)
        f.frame = NSRect(x: ox, y: oy, width: pw, height: 16)
        f.isEnabled = false
        iv.addSubview(f)
        return f
    }

    func createWindow() {
        let imgH: CGFloat = 230, imgW: CGFloat = 229
        let s = dispW / imgW, imgDH = imgH * s
        let winW = dispW + 40, winH = imgDH + 80
        let w = NSWindow(contentRect: NSRect(x: 0, y: 0, width: winW, height: winH),
                         styleMask: [.titled, .closable, .miniaturizable],
                         backing: .buffered, defer: false)
        w.title = "Hagibis Mapping"; w.center(); w.isReleasedWhenClosed = false; w.delegate = self
        let cv = w.contentView!
        let iv = NSImageView(frame: NSRect(x: 20, y: 60, width: dispW, height: imgDH))
        iv.imageScaling = .scaleProportionallyUpOrDown
        if let rp = Bundle.main.resourcePath { iv.image = NSImage(contentsOfFile: rp + "/hub.png") }
        if iv.image == nil {
            let execDir = URL(fileURLWithPath: CommandLine.arguments[0]).deletingLastPathComponent().path
            iv.image = NSImage(contentsOfFile: execDir + "/hub.png")
        }
        if iv.image == nil { iv.wantsLayer = true; iv.layer?.backgroundColor = NSColor.darkGray.cgColor }
        cv.addSubview(iv)

        let pad = 0.10
        btnTL   = makePill(pad, 0.12, false, dispW, "button_top_left", iv, imgDH, s)
        btnTLh  = makePill(pad, 0.22, false, dispW, "button_top_left_hold", iv, imgDH, s)
        btnBR   = makePill(1 - pad, 0.78, true, dispW, "button_bottom_right", iv, imgDH, s)
        btnBRh  = makePill(1 - pad, 0.88, true, dispW, "button_bottom_right_hold", iv, imgDH, s)
        knobCCW = makePill(0.22, 0.48, false, dispW, "knob_ccw", iv, imgDH, s)
        knobCW  = makePill(0.54, 0.48, false, dispW, "knob_cw", iv, imgDH, s)
        knobClick = makePill(0.38, 0.58, false, dispW, "knob_click", iv, imgDH, s)
        playField = makePill(1 - pad, 0.12, true, dispW, "play_pause", iv, imgDH, s)

        // Single gesture recognizer on the parent image view — hit-test
        // the background subview that was clicked. This avoids the
        // NSTextField-interception problem entirely.
        let rec = NSClickGestureRecognizer(target: self, action: #selector(pillClicked))
        iv.addGestureRecognizer(rec)

        statusField = NSTextField(labelWithString: "● Running")
        statusField.font = .systemFont(ofSize: 12, weight: .semibold)
        statusField.frame = NSRect(x: 20, y: 42, width: winW - 40, height: 16); cv.addSubview(statusField)
        appField = NSTextField(labelWithString: "—")
        appField.font = .systemFont(ofSize: 11); appField.textColor = .secondaryLabelColor
        appField.frame = NSRect(x: 220, y: 42, width: 200, height: 16); cv.addSubview(appField)
        let btn = NSButton(title: "Stop", target: self, action: #selector(toggleEngine))
        btn.bezelStyle = .rounded; btn.controlSize = .regular
        btn.frame = NSRect(x: 20, y: 10, width: 80, height: 28); cv.addSubview(btn)
        window = w
    }

    @objc func pillClicked(_ sender: NSClickGestureRecognizer) {
        guard let iv = sender.view as? NSImageView else { return }
        let pt = sender.location(in: iv)
        // Iterate subviews in reverse (front-to-back) to find the hit bg
        for sv in iv.subviews.reversed() where sv.toolTip != nil && !(sv.toolTip?.isEmpty ?? true) {
            if sv.frame.contains(pt) {
                edKey = sv.toolTip!
                openEditor()
                return
            }
        }
    }

// ── About / Quit ─────────────────────────────────────────────────────

    @objc func showAbout() {
        NSApp.setActivationPolicy(.regular)
        NSApp.orderFrontStandardAboutPanel(nil)
        NSApp.activate(ignoringOtherApps: true)
    }
    @objc func quitApp() { timer?.invalidate(); timer = nil; hagibis_stop(); NSApp.terminate(nil) }
    func windowShouldClose(_ s: NSWindow) -> Bool { s.orderOut(nil); NSApp.setActivationPolicy(.accessory); return false }
}

// ── Config helpers ─────────────────────────────────────────────────────

extension AppDelegate {

    func myLog(_ level: Int32, _ cat: String, _ msg: String) {
        cat.withCString { cCat in msg.withCString { cMsg in hagibis_log(level, cCat, cMsg) } }
    }
    func readConfig() -> [String: Any]? {
        var buf = [CChar](repeating: 0, count: 65536)
        let len = buf.withUnsafeMutableBufferPointer { ptr -> Int32 in
            guard let addr = ptr.baseAddress else { return 0 }
            return hagibis_config_json(addr, 65536)
        }
        guard len > 0,
              let json = String(bytes: Data(bytes: buf, count: Int(len)), encoding: .utf8),
              let data = json.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return nil }
        return obj
    }

    func writeConfig(_ dict: [String: Any]) -> Bool {
        guard let data = try? JSONSerialization.data(withJSONObject: dict, options: []),
              let json = String(data: data, encoding: .utf8)
        else { return false }
        let ok = json.withCString { ptr in hagibis_save_config_json(ptr) } == 0
        if ok { _ = hagibis_reload_config() }
        return ok
    }

    func openApps() -> [(String, String)] {
        NSWorkspace.shared.runningApplications
            .filter { $0.activationPolicy == .regular && $0.bundleIdentifier != nil }
            .compactMap { app in
                guard let bid = app.bundleIdentifier, !bid.isEmpty else { return nil }
                return (bid, app.localizedName ?? bid)
            }
    }

    func appChoices() -> [(id: String, name: String)] {
        var seen: Set<String> = []
        var out: [(String, String)] = []
        for (bid, name) in openApps() { if seen.insert(bid).inserted { out.append((bid, name)) } }
        if let cfg = readConfig(),
           let profiles = cfg["profiles"] as? [[String: Any]] {
            for p in profiles {
                guard let bid = p["app_id"] as? String, !bid.isEmpty else { continue }
                if seen.insert(bid).inserted {
                    out.append((bid, p["app_name"] as? String ?? bid))
                }
            }
        }
        return out
    }
}

// ── Binding Editor ─────────────────────────────────────────────────────

struct MediaKeyItem { let label: String; let keyType: UInt8 }
struct MouseItem { let label: String; let type: String; let btn: UInt8; let dx: Double; let dy: Double }

let mediaKeys: [MediaKeyItem] = [
    MediaKeyItem(label: "Vol+", keyType: 0), MediaKeyItem(label: "Vol−", keyType: 1), MediaKeyItem(label: "Mute", keyType: 7),
    MediaKeyItem(label: "Play/Pause", keyType: 16), MediaKeyItem(label: "Next Track", keyType: 17), MediaKeyItem(label: "Previous Track", keyType: 18),
    MediaKeyItem(label: "Brightness+", keyType: 2), MediaKeyItem(label: "Brightness−", keyType: 3), MediaKeyItem(label: "Eject", keyType: 14),
]

let mouseGestures: [MouseItem] = [
    MouseItem(label: "Left Click", type: "MouseClick", btn: 1, dx: 0, dy: 0),
    MouseItem(label: "Right Click", type: "MouseClick", btn: 2, dx: 0, dy: 0),
    MouseItem(label: "Middle Click", type: "MouseClick", btn: 3, dx: 0, dy: 0),
    MouseItem(label: "Scroll Up", type: "MouseScroll", btn: 0, dx: 0, dy: 3),
    MouseItem(label: "Scroll Down", type: "MouseScroll", btn: 0, dx: 0, dy: -3),
]

extension AppDelegate {

    func openEditor() {
        edConfig = readConfig() ?? [:]
        guard !edConfig.isEmpty, !edKey.isEmpty else { return }

        let winW: CGFloat = 420, winH: CGFloat = 360
        let win = NSWindow(contentRect: NSRect(x: 0, y: 0, width: winW, height: winH),
                           styleMask: [.titled, .closable],
                           backing: .buffered, defer: false)
        win.title = "Edit Binding"
        win.center(); win.isReleasedWhenClosed = false
        let cv = win.contentView!

        // App dropdown
        let appLbl = NSTextField(labelWithString: "Application:")
        appLbl.frame = NSRect(x: 20, y: winH - 44, width: 80, height: 16)
        cv.addSubview(appLbl)
        edAppDropdown = NSPopUpButton(frame: NSRect(x: 105, y: winH - 50, width: 230, height: 24))
        edAppDropdown.addItem(withTitle: "Default")
        for (bid, name) in appChoices() {
            edAppDropdown.addItem(withTitle: name)
            edAppDropdown.lastItem?.representedObject = bid
        }
        cv.addSubview(edAppDropdown)
        edAppDropdown.target = self; edAppDropdown.action = #selector(edAppChanged)
        let hint = NSTextField(labelWithString: "Open the app if not listed.")
        hint.font = .systemFont(ofSize: 10); hint.textColor = .secondaryLabelColor
        hint.frame = NSRect(x: 20, y: winH - 68, width: 380, height: 14)
        cv.addSubview(hint)

        // Control title
        let title = NSTextField(labelWithString: "Control: \(edKey.replacingOccurrences(of: "_", with: " ").capitalized)")
        title.font = .systemFont(ofSize: 11, weight: .medium)
        title.frame = NSRect(x: 20, y: winH - 95, width: 380, height: 16)
        cv.addSubview(title)

        // ── Radio buttons vertically stacked, fields to their right ─────
        let radioX: CGFloat = 20, fieldX: CGFloat = 170, rowH: CGFloat = 28
        let startY = winH - 130

        edRadioKey = NSButton(radioButtonWithTitle: "Keyboard Shortcut", target: self, action: #selector(edTypeChanged))
        edRadioKey.frame = NSRect(x: radioX, y: startY, width: 145, height: rowH); cv.addSubview(edRadioKey)

        edRadioMedia = NSButton(radioButtonWithTitle: "Media Key", target: self, action: #selector(edTypeChanged))
        edRadioMedia.frame = NSRect(x: radioX, y: startY - rowH, width: 145, height: rowH); cv.addSubview(edRadioMedia)

        edRadioMouse = NSButton(radioButtonWithTitle: "Mouse Gesture", target: self, action: #selector(edTypeChanged))
        edRadioMouse.frame = NSRect(x: radioX, y: startY - rowH * 2, width: 145, height: rowH); cv.addSubview(edRadioMouse)

        // Ensure mutual exclusion: all three call the same action.
        // iOS-style grouping via superview works on macOS when same action + same target.

        // Key combo field (aligned with "Keyboard Shortcut" row)
        edKeyField = KeyCaptureField(frame: NSRect(x: fieldX, y: startY + 3, width: 140, height: 22))
        edKeyField.isBordered = true; edKeyField.font = .monospacedDigitSystemFont(ofSize: 11, weight: .regular)
        edKeyField.backgroundColor = .controlBackgroundColor
        cv.addSubview(edKeyField)

        // Media key dropdown (aligned with "Media Key" row)
        edMediaDropdown = NSPopUpButton(frame: NSRect(x: fieldX, y: startY - rowH - 1, width: 140, height: 22))
        for mk in mediaKeys { edMediaDropdown.addItem(withTitle: mk.label) }
        edMediaDropdown.isEnabled = false
        cv.addSubview(edMediaDropdown)

        // Mouse gesture dropdown (aligned with "Mouse Gesture" row)
        edMouseDropdown = NSPopUpButton(frame: NSRect(x: fieldX, y: startY - rowH * 2 - 1, width: 140, height: 22))
        for mg in mouseGestures { edMouseDropdown.addItem(withTitle: mg.label) }
        edMouseDropdown.isEnabled = false
        cv.addSubview(edMouseDropdown)

        // Optional label
        edLabelField = NSTextField(frame: NSRect(x: fieldX + 150, y: startY + 3, width: 90, height: 22))
        edLabelField.isBordered = true; edLabelField.font = .systemFont(ofSize: 11)
        edLabelField.placeholderString = "Label"
        cv.addSubview(edLabelField)

        // Default: Keyboard selected
        edRadioKey.state = .on
        edCurType = "Keyboard"

        // Save / Cancel
        let cancelBtn = NSButton(title: "Cancel", target: self, action: #selector(closeEditor))
        cancelBtn.bezelStyle = .rounded; cancelBtn.controlSize = .regular
        cancelBtn.frame = NSRect(x: winW - 180, y: 12, width: 80, height: 28)
        cv.addSubview(cancelBtn)
        let saveBtn = NSButton(title: "Save", target: self, action: #selector(saveAndCloseEditor))
        saveBtn.bezelStyle = .rounded; saveBtn.controlSize = .regular
        saveBtn.frame = NSRect(x: winW - 95, y: 12, width: 80, height: 28)
        cv.addSubview(saveBtn)

        editor = win
        self.window?.addChildWindow(win, ordered: .above)
        win.makeKeyAndOrderFront(nil)

        populateEditor(initial: true)
        myLog(2, "editor", "opened for key=\(edKey)")
    }

    @objc func edTypeChanged(_ sender: NSButton) {
        edRadioKey.state = (sender === edRadioKey) ? .on : .off
        edRadioMedia.state = (sender === edRadioMedia) ? .on : .off
        edRadioMouse.state = (sender === edRadioMouse) ? .on : .off

        let isKey = sender === edRadioKey
        let isMedia = sender === edRadioMedia
        edCurType = isKey ? "Keyboard" : isMedia ? "MediaKey" : "MouseClick"

        // Ghost inactive rows (isEnabled=false), don't hide them
        edKeyField.isEnabled = isKey
        edMediaDropdown.isEnabled = isMedia
        edMouseDropdown.isEnabled = !isKey && !isMedia
    }

    @objc func edAppChanged() { populateEditor() }

    func populateEditor(initial: Bool = false) {
        guard let profiles = edConfig["profiles"] as? [[String: Any]],
              let defaults = edConfig["default"] as? [String: Any]
        else { return }

        // On initial load, set dropdown to the focused app (if it has a
        // profile) or Default. On user-driven change, keep the dropdown
        // where the user left it.
        if initial {
            var found = false
            if !focusedAppId.isEmpty {
                for i in 1..<edAppDropdown.numberOfItems {
                    if edAppDropdown.item(at: i)?.representedObject as? String == focusedAppId {
                        edAppDropdown.selectItem(at: i)
                        found = true
                        break
                    }
                }
            }
            if !found { edAppDropdown.selectItem(at: 0) }
        }

        // Read selected app from dropdown (after any initial-position fix)
        let selIdx = edAppDropdown.indexOfSelectedItem
        let selAppId: String? = selIdx > 0
            ? (edAppDropdown.selectedItem?.representedObject as? String)
            : nil

        // Look up mapping: profile for selected app, else defaults
        var evt: [String: Any]?
        if let bid = selAppId,
           let profile = profiles.first(where: { ($0["app_id"] as? String) == bid }),
           let m = profile["mappings"] as? [String: Any],
           let e = m[edKey] as? [String: Any] {
            evt = e
        } else {
            evt = defaults[edKey] as? [String: Any]
        }

        guard let evt, let type = evt["type"] as? String else {
            // No mapping exists for this key — clear fields
            edCurType = "Keyboard"
            edRadioKey.state = .on; edRadioMedia.state = .off; edRadioMouse.state = .off
            edKeyField.isEnabled = true; edKeyField.stringValue = ""
            edMediaDropdown.isEnabled = false
            edMouseDropdown.isEnabled = false
            edLabelField.stringValue = ""
            return
        }

        edCurType = type
        if type == "Keyboard" {
            edRadioKey.state = .on; edRadioMedia.state = .off; edRadioMouse.state = .off
            edKeyField.isEnabled = true
            edMediaDropdown.isEnabled = false
            edMouseDropdown.isEnabled = false
            edKeyField.stringValue = evt["binding"] as? String ?? ""
        } else if type == "MediaKey" {
            edRadioKey.state = .off; edRadioMedia.state = .on; edRadioMouse.state = .off
            edKeyField.isEnabled = false
            edMediaDropdown.isEnabled = true
            edMouseDropdown.isEnabled = false
            if let kt = evt["key_type"] as? Int,
               let idx = mediaKeys.firstIndex(where: { $0.keyType == UInt8(truncatingIfNeeded: kt) }) {
                edMediaDropdown.selectItem(at: idx)
            }
        } else if type == "MouseClick" || type == "MouseScroll" {
            edRadioKey.state = .off; edRadioMedia.state = .off; edRadioMouse.state = .on
            edKeyField.isEnabled = false
            edMediaDropdown.isEnabled = false
            edMouseDropdown.isEnabled = true
            if type == "MouseClick", let btn = evt["button"] as? Int {
                if let idx = mouseGestures.firstIndex(where: { $0.btn == UInt8(truncatingIfNeeded: btn) }) {
                    edMouseDropdown.selectItem(at: idx)
                }
            } else if type == "MouseScroll", let dy = evt["dy"] as? Double {
                if dy > 0, let idx = mouseGestures.firstIndex(where: { $0.label == "Scroll Up" }) { edMouseDropdown.selectItem(at: idx) }
                else if dy < 0, let idx = mouseGestures.firstIndex(where: { $0.label == "Scroll Down" }) { edMouseDropdown.selectItem(at: idx) }
            }
        }
        edLabelField.stringValue = evt["label"] as? String ?? ""
    }

    @objc func closeEditor() {
        myLog(2, "editor", "closed without save")
        editor?.close(); editor = nil
    }

    @objc func saveAndCloseEditor() {
        guard let chosenAppName = edAppDropdown.selectedItem?.title else { closeEditor(); return }
        let chosenAppId: String? = edAppDropdown.selectedItem?.title == "Default" ? nil : (edAppDropdown.selectedItem?.representedObject as? String)

        var evt: [String: Any] = ["type": edCurType]
        if let l = edLabelField?.stringValue, !l.isEmpty { evt["label"] = l }

        switch edCurType {
        case "Keyboard":
            guard let b = edKeyField?.stringValue, !b.isEmpty else { closeEditor(); return }
            evt["binding"] = b
        case "MediaKey":
            let idx = edMediaDropdown.indexOfSelectedItem
            guard idx >= 0, idx < mediaKeys.count else { closeEditor(); return }
            evt["key_type"] = Int(mediaKeys[idx].keyType)
        default:
            let idx = edMouseDropdown.indexOfSelectedItem
            guard idx >= 0, idx < mouseGestures.count else { closeEditor(); return }
            let mg = mouseGestures[idx]
            evt["type"] = mg.type
            if mg.type == "MouseClick" { evt["button"] = Int(mg.btn) }
            else if mg.type == "MouseScroll" { evt["dx"] = 0.0; evt["dy"] = mg.dy }
        }

        // Apply to config dict
        if chosenAppId == nil {
            if var d = edConfig["default"] as? [String: Any] {
                d[edKey] = evt
                edConfig["default"] = d
            }
        } else {
            var profiles = edConfig["profiles"] as? [[String: Any]] ?? []
            if let idx = profiles.firstIndex(where: { ($0["app_id"] as? String) == chosenAppId }) {
                if var m = profiles[idx]["mappings"] as? [String: Any] {
                    m[edKey] = evt
                    profiles[idx]["mappings"] = m
                }
            } else {
                let newProfile: [String: Any] = [
                    "app_id": chosenAppId!,
                    "app_name": chosenAppName,
                    "mappings": [edKey: evt],
                ]
                profiles.append(newProfile)
            }
            edConfig["profiles"] = profiles
        }

        myLog(2, "editor", "saving key=\(edKey) app=\(chosenAppName) type=\(edCurType)")
        _ = writeConfig(edConfig)
        closeEditor()
    }
}

// ── App entry ───────────────────────────────────────────────────────────

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.setActivationPolicy(.accessory)
app.run()
