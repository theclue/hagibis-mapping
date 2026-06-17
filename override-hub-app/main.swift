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

if geteuid() != 0 {
    if let execPath = Bundle.main.executablePath {
        _ = execPath.withCString { hagibis_elevate($0) }
    }
    exit(0)
}

// ── Key capture text field ───────────────────────────────────────────────

class KeyCaptureField: NSTextField {
    var onKeyCombo: ((String) -> Void)?

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        interpret(event)
        return true
    }

    override func keyDown(with event: NSEvent) {
        interpret(event)
    }

    override func flagsChanged(with event: NSEvent) {
        // Modifier-only changes — ignored unless keys are also pressed
    }

    private func interpret(_ event: NSEvent) {
        var parts: [String] = []
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        if flags.contains(.control) { parts.append("Ctrl") }
        if flags.contains(.shift) { parts.append("Shift") }
        if flags.contains(.option) { parts.append("Alt") }
        if flags.contains(.command) { parts.append("Cmd") }

        let key = event.charactersIgnoringModifiers?.uppercased() ?? ""
        // Map special keys to readable names
        let mapped: String? = switch event.keyCode {
            case 36: "Enter"
            case 48: "Tab"
            case 49: "Space"
            case 51: "Backspace"
            case 53: "Escape"
            case 117: "Delete"
            case 115: "Home"
            case 119: "End"
            case 116: "PageUp"
            case 121: "PageDown"
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

// ── App Delegate ─────────────────────────────────────────────────────────

class AppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate {
    var statusItem: NSStatusItem!
    var timer: Timer?
    var window: NSWindow?          // monitor window
    var editor: NSWindow?          // binding-editor window

    var statusField: NSTextField!, appField: NSTextField!
    var btnTL: NSTextField!, btnTLh: NSTextField!, btnBR: NSTextField!, btnBRh: NSTextField!
    var knobCW: NSTextField!, knobCCW: NSTextField!, knobClick: NSTextField!
    var playField: NSTextField!

    // Editor controls
    var edAppDropdown: NSPopUpButton!
    var edControlLabel: NSTextField!

    // ── editor state ──
    var edKey: String = ""           // toml key being edited
    var edSingle: Bool = true        // true = single event, false = press+hold
    var edConfig: [String: Any] = [:]

    // ── Press + Hold layout ──
    var edPressRadio: NSButton!, edPressKeyField: KeyCaptureField!, edPressMediaDropdown: NSPopUpButton!, edPressMouseDropdown: NSPopUpButton!, edPressLabel: NSTextField!
    var edHoldRadio: NSButton!, edHoldKeyField: KeyCaptureField!, edHoldMediaDropdown: NSPopUpButton!, edHoldMouseDropdown: NSPopUpButton!, edHoldLabel: NSTextField!

    // radio group helpers
    var pressType: String = "Keyboard"
    var holdType: String = "Keyboard"

    let devTL = "Cmd+R", devTLh = "Ctrl+Q", devBR = "Cmd+F13", devBRh = "Ctrl+3"
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
        if hagibis_start() != 0 {
            NSLog("[Hagibis Mapping] hagibis_start() failed")
        }
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

        let isOn = { (code: UInt8) -> Bool in keys.contains(code) }
        let d = { (on: Bool) -> String in on ? "●" : "○" }

        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.focusedAppId = appId
            let enginePresent = s["engine_present"] as? Bool ?? false
            self.statusItem?.button?.title = running ? "●HM" : "○HM"
            if let tmi = self.statusItem?.menu?.item(at: 0) {
                tmi.title = enginePresent ? "Stop" : "Start"
            }
            self.statusField?.stringValue = running ? "● Running" : "○ Stopped"
            self.appField?.stringValue = app

            let tl = running ? lblTL : self.devTL, tl_h = running ? lblTLh : self.devTLh
            let br = running ? lblBR : self.devBR, br_h = running ? lblBRh : self.devBRh

            self.btnTL?.stringValue = "\(d(isOn(0x0F))) \(tl)"
            self.btnTL?.textColor = isOn(0x0F) ? .systemGreen : NSColor(white: 0.15, alpha: 1)
            self.btnTLh?.stringValue = "\(d(isOn(0x14))) \(tl_h)"
            self.btnTLh?.textColor = isOn(0x14) ? .systemGreen : NSColor(white: 0.15, alpha: 1)
            self.btnBR?.stringValue = "\(d(isOn(0x46))) \(br)"
            self.btnBR?.textColor = isOn(0x46) ? .systemGreen : NSColor(white: 0.15, alpha: 1)
            self.btnBRh?.stringValue = "\(d(isOn(0x20))) \(br_h)"
            self.btnBRh?.textColor = isOn(0x20) ? .systemGreen : NSColor(white: 0.15, alpha: 1)
            let cwLabel = running ? lblCW : "CW", ccwLabel = running ? lblCCW : "CCW"
            self.knobCW?.stringValue = "\(d(cons & 0x01 != 0)) \(cwLabel)"
            self.knobCW?.textColor = cons & 0x01 != 0 ? .systemGreen : NSColor(white: 0.15, alpha: 1)
            self.knobCCW?.stringValue = "\(d(cons & 0x02 != 0)) \(ccwLabel)"
            self.knobCCW?.textColor = cons & 0x02 != 0 ? .systemGreen : NSColor(white: 0.15, alpha: 1)
            let clickLabel = running ? lblClick : "Click"
            self.knobClick?.stringValue = "\(d(cons & 0x04 != 0)) \(clickLabel)"
            self.knobClick?.textColor = cons & 0x04 != 0 ? .systemGreen : NSColor(white: 0.15, alpha: 1)
            let playLabel = running ? lblPlay : "Play"
            self.playField?.stringValue = "\(d(cons & 0x40 != 0)) \(playLabel)"
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
        let pw: CGFloat = 65 * s; let pad: Double = 0.10
        let ox: CGFloat = ra ? CGFloat(1 - pad) * w - pw : CGFloat(xf) * w
        let oy = imgDH - CGFloat(yf) * imgDH - 14
        let bg = NSView(frame: NSRect(x: ox - 4, y: oy - 1, width: pw + 8, height: 18))
        bg.wantsLayer = true; bg.layer?.backgroundColor = NSColor(white: 1, alpha: 0.8).cgColor
        bg.layer?.cornerRadius = 4
        iv.addSubview(bg)
        // Click handler via tag
        let rec = NSClickGestureRecognizer(target: self, action: #selector(pillClicked))
        bg.addGestureRecognizer(rec)
        // Store the binding key in the tooltip so the handler can read it
        bg.toolTip = tag

        let f = NSTextField(labelWithString: "○ —")
        f.font = .monospacedDigitSystemFont(ofSize: 10, weight: .regular)
        f.textColor = NSColor(white: 0.15, alpha: 1)
        f.frame = NSRect(x: ox, y: oy, width: pw, height: 16)
        f.isEnabled = false  // let clicks pass through to the bg gesture recognizer
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
        if let rp = Bundle.main.resourcePath {
            iv.image = NSImage(contentsOfFile: rp + "/hub.png")
        }
        if iv.image == nil {
            let execDir = URL(fileURLWithPath: CommandLine.arguments[0]).deletingLastPathComponent().path
            iv.image = NSImage(contentsOfFile: execDir + "/hub.png")
        }
        if iv.image == nil { iv.wantsLayer = true; iv.layer?.backgroundColor = NSColor.darkGray.cgColor }
        cv.addSubview(iv)

        let pad = 0.10
        btnTL   = makePill(pad, 0.12, false, dispW, "button_top_left,button_top_left_hold", iv, imgDH, s)
        btnTLh  = makePill(pad, 0.22, false, dispW, "button_top_left_hold", iv, imgDH, s)
        btnBR   = makePill(1 - pad, 0.78, true, dispW, "button_bottom_right,button_bottom_right_hold", iv, imgDH, s)
        btnBRh  = makePill(1 - pad, 0.88, true, dispW, "button_bottom_right_hold", iv, imgDH, s)
        // Knob: CCW (left) + CW (right) + Click (centered below)
        knobCCW = makePill(0.22, 0.48, false, dispW, "knob_ccw", iv, imgDH, s)
        knobCW  = makePill(0.54, 0.48, false, dispW, "knob_cw", iv, imgDH, s)
        knobClick = makePill(0.38, 0.58, false, dispW, "knob_click", iv, imgDH, s)
        playField = makePill(1 - pad, 0.12, true, dispW, "play_pause", iv, imgDH, s)

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
        guard let bg = sender.view, let keyTag = bg.toolTip, !keyTag.isEmpty else { return }
        openEditor(for: keyTag.components(separatedBy: ","))
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

// ── Config helpers ──────────────────────────────────────────────────────

extension AppDelegate {

    /// Read the current config as a Swift dict via FFI.
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

    /// Write config dict via FFI, then trigger engine reload.
    func writeConfig(_ dict: [String: Any]) -> Bool {
        guard let data = try? JSONSerialization.data(withJSONObject: dict, options: []),
              let json = String(data: data, encoding: .utf8)
        else { return false }
        let ok = json.withCString { ptr in hagibis_save_config_json(ptr) } == 0
        if ok { _ = hagibis_reload_config() }
        return ok
    }

    /// List of open GUI apps: [(bundleId, localizedName)]
    func openApps() -> [(String, String)] {
        NSWorkspace.shared.runningApplications
            .filter { $0.activationPolicy == .regular && $0.bundleIdentifier != nil }
            .compactMap { app in
                guard let bid = app.bundleIdentifier, !bid.isEmpty else { return nil }
                let name = app.localizedName ?? bid
                return (bid, name)
            }
    }

    /// Union: open apps + TOML profiles.
    func appChoices() -> [(id: String, name: String)] {
        var seen: Set<String> = []
        var out: [(String, String)] = []
        for (bid, name) in openApps() {
            if seen.insert(bid).inserted { out.append((bid, name)) }
        }
        if let cfg = readConfig(),
           let profiles = cfg["profiles"] as? [[String: Any]] {
            for p in profiles {
                guard let bid = p["app_id"] as? String, !bid.isEmpty else { continue }
                if seen.insert(bid).inserted {
                    let name = p["app_name"] as? String ?? bid
                    out.append((bid, name))
                }
            }
        }
        return out
    }
}

// ── Binding Editor ──────────────────────────────────────────────────────

struct MediaKeyItem {
    let label: String; let keyType: UInt8
}

struct MouseItem {
    let label: String; let type: String; let btn: UInt8; let dx: Double; let dy: Double
}

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

    func openEditor(for keys: [String]) {
        edConfig = readConfig() ?? [:]
        guard !edConfig.isEmpty else { return }
        edKey = keys.first ?? ""
        edSingle = keys.count == 1

        // ── Build the window ──────────────────────────────────────────────
        let win = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 500, height: edSingle ? 290 : 380),
                           styleMask: [.titled, .closable],
                           backing: .buffered, defer: false)
        win.title = "Edit Binding"
        win.center(); win.isReleasedWhenClosed = false
        let cv = win.contentView!

        // App dropdown
        let appLbl = NSTextField(labelWithString: "Application:")
        appLbl.frame = NSRect(x: 20, y: win.frame.height - 50, width: 80, height: 16)
        cv.addSubview(appLbl)

        edAppDropdown = NSPopUpButton(frame: NSRect(x: 105, y: win.frame.height - 56, width: 280, height: 24))
        edAppDropdown.addItem(withTitle: "Default")
        for (bid, name) in appChoices() {
            edAppDropdown.addItem(withTitle: name)
            edAppDropdown.lastItem?.representedObject = bid
        }
        cv.addSubview(edAppDropdown)

        let hint = NSTextField(labelWithString: "Open the app you want to map if not in the list.")
        hint.font = .systemFont(ofSize: 10); hint.textColor = .secondaryLabelColor
        hint.frame = NSRect(x: 390, y: win.frame.height - 50, width: 100, height: 30)
        cv.addSubview(hint)

        // Control title
        edControlLabel = NSTextField(labelWithString: "Control: \(edKey.replacingOccurrences(of: "_", with: " ").capitalized)")
        edControlLabel.font = .systemFont(ofSize: 11, weight: .medium)
        edControlLabel.frame = NSRect(x: 20, y: win.frame.height - 80, width: 200, height: 16)
        cv.addSubview(edControlLabel)

        // ── Build Press row (and Hold row if dual) ────────────────────────
        let y1 = win.frame.height - 110
        buildEventRow(into: cv, y: y1, label: edSingle ? "" : "Short Press",
                      key: keys.first ?? "", win: win, isPress: true)

        if !edSingle {
            let y2 = win.frame.height - 230
            buildEventRow(into: cv, y: y2, label: "Hold",
                          key: keys.count > 1 ? keys[1] : "", win: win, isPress: false)
        }

        // Save / Cancel
        let cancelBtn = NSButton(title: "Cancel", target: self, action: #selector(closeEditor))
        cancelBtn.bezelStyle = .rounded; cancelBtn.controlSize = .regular
        cancelBtn.frame = NSRect(x: win.frame.width - 180, y: 16, width: 80, height: 28)
        cv.addSubview(cancelBtn)

        let saveBtn = NSButton(title: "Save", target: self, action: #selector(saveAndCloseEditor))
        saveBtn.bezelStyle = .rounded; saveBtn.controlSize = .regular
        saveBtn.frame = NSRect(x: win.frame.width - 95, y: 16, width: 80, height: 28)
        cv.addSubview(saveBtn)

        editor = win
        self.window?.addChildWindow(win, ordered: .above)  // stays above monitor
        win.makeKeyAndOrderFront(nil)

        // Pre-populate with current mapping
        populateEditor()
    }

    func buildEventRow(into cv: NSView, y: CGFloat, label: String, key: String,
                       win: NSWindow, isPress: Bool) {
        if !label.isEmpty {
            let l = NSTextField(labelWithString: label)
            l.font = .systemFont(ofSize: 10, weight: .semibold)
            l.frame = NSRect(x: 20, y: y + 8, width: 80, height: 14)
            cv.addSubview(l)
        }

        let radioY = y - 6
        let r1 = NSButton(radioButtonWithTitle: "Key", target: self, action: isPress ? #selector(pressTypeChanged) : #selector(holdTypeChanged))
        r1.frame = NSRect(x: 20, y: radioY, width: 55, height: 18); cv.addSubview(r1)

        let r2 = NSButton(radioButtonWithTitle: "Media", target: self, action: isPress ? #selector(pressTypeChanged) : #selector(holdTypeChanged))
        r2.frame = NSRect(x: 78, y: radioY, width: 65, height: 18); cv.addSubview(r2)

        let r3 = NSButton(radioButtonWithTitle: "Mouse", target: self, action: isPress ? #selector(pressTypeChanged) : #selector(holdTypeChanged))
        r3.frame = NSRect(x: 146, y: radioY, width: 70, height: 18); cv.addSubview(r3)

        let kf = KeyCaptureField(frame: NSRect(x: 220, y: radioY + 1, width: 120, height: 20))
        kf.isBordered = true; kf.font = .monospacedDigitSystemFont(ofSize: 11, weight: .regular)
        kf.backgroundColor = .controlBackgroundColor
        cv.addSubview(kf)

        let md = NSPopUpButton(frame: NSRect(x: 220, y: radioY - 2, width: 120, height: 22))
        for mk in mediaKeys { md.addItem(withTitle: mk.label) }
        cv.addSubview(md)

        let gd = NSPopUpButton(frame: NSRect(x: 220, y: radioY - 2, width: 120, height: 22))
        for mg in mouseGestures { gd.addItem(withTitle: mg.label) }
        cv.addSubview(gd)

        let lbl = NSTextField(frame: NSRect(x: 348, y: radioY + 1, width: 130, height: 20))
        lbl.isBordered = true; lbl.font = .systemFont(ofSize: 11); lbl.placeholderString = "Label (optional)"
        cv.addSubview(lbl)

        if isPress {
            edPressRadio = r1; edPressKeyField = kf; edPressMediaDropdown = md
            edPressMouseDropdown = gd; edPressLabel = lbl
        } else {
            edHoldRadio = r1; edHoldKeyField = kf; edHoldMediaDropdown = md
            edHoldMouseDropdown = gd; edHoldLabel = lbl
        }

        // Default: select Keyboard
        r1.state = .on
        md.isHidden = true
        gd.isHidden = true
    }

    @objc func pressTypeChanged() {
        if edPressRadio.state == .on  { pressType = "Keyboard"; edPressKeyField.isHidden = false; edPressMediaDropdown.isHidden = true; edPressMouseDropdown.isHidden = true }
        else if edPressMediaDropdown.isHidden == false { pressType = "MediaKey"; edPressKeyField.isHidden = true; edPressMediaDropdown.isHidden = false; edPressMouseDropdown.isHidden = true }
        else { pressType = "MouseClick"; edPressKeyField.isHidden = true; edPressMediaDropdown.isHidden = true; edPressMouseDropdown.isHidden = false }
    }

    @objc func holdTypeChanged() {
        if edHoldRadio.state == .on  { holdType = "Keyboard"; edHoldKeyField.isHidden = false; edHoldMediaDropdown.isHidden = true; edHoldMouseDropdown.isHidden = true }
        else if edHoldMediaDropdown.isHidden == false { holdType = "MediaKey"; edHoldKeyField.isHidden = true; edHoldMediaDropdown.isHidden = false; edHoldMouseDropdown.isHidden = true }
        else { holdType = "MouseClick"; edHoldKeyField.isHidden = true; edHoldMediaDropdown.isHidden = true; edHoldMouseDropdown.isHidden = false }
    }

    func populateEditor() {
        guard let profiles = edConfig["profiles"] as? [[String: Any]],
              let defaults = edConfig["default"] as? [String: Any]
        else { return }

        // Find the currently-focused app in profiles
        if let idx = profiles.firstIndex(where: { ($0["app_id"] as? String) == focusedAppId }) {
            edAppDropdown.selectItem(at: idx + 1)
        }

        func loadMapping(_ key: String) -> [String: Any]? {
            if let idx = profiles.firstIndex(where: { ($0["app_id"] as? String) == focusedAppId }),
               let m = profiles[idx]["mappings"] as? [String: Any],
               let evt = m[key] as? [String: Any] {
                return evt
            }
            return defaults[key] as? [String: Any]
        }

        if let evt = loadMapping(edKey) {
            populateRow(evt, isPress: true)
        }
        if !edSingle, let keys2 = edKey.components(separatedBy: ",").last,
           let evt = loadMapping(keys2) {
            populateRow(evt, isPress: false)
        }
    }

    func populateRow(_ evt: [String: Any], isPress: Bool) {
        let kf = isPress ? edPressKeyField : edHoldKeyField
        let md = isPress ? edPressMediaDropdown : edHoldMediaDropdown
        let gd = isPress ? edPressMouseDropdown : edHoldMouseDropdown
        let lbl = isPress ? edPressLabel : edHoldLabel
        guard let type = evt["type"] as? String else { return }

        if let l = evt["label"] as? String, !l.isEmpty { lbl?.stringValue = l }

        switch type {
        case "Keyboard":
            if let b = evt["binding"] as? String { kf?.stringValue = b }
        case "MediaKey":
            if let kt = evt["key_type"] as? Int {
                if let idx = mediaKeys.firstIndex(where: { $0.keyType == UInt8(truncatingIfNeeded: kt) }) {
                    md?.selectItem(at: idx)
                }
            }
        case "MouseClick":
            if let btn = evt["button"] as? Int {
                if let idx = mouseGestures.firstIndex(where: { $0.btn == UInt8(truncatingIfNeeded: btn) }) {
                    gd?.selectItem(at: idx)
                }
            }
        case "MouseScroll":
            if let dy = evt["dy"] as? Double {
                if dy > 0, let idx = mouseGestures.firstIndex(where: { $0.label == "Scroll Up" }) { gd?.selectItem(at: idx) }
                else if dy < 0, let idx = mouseGestures.firstIndex(where: { $0.label == "Scroll Down" }) { gd?.selectItem(at: idx) }
            }
        default: break
        }
    }

    @objc func closeEditor() {
        editor?.close(); editor = nil
    }

    @objc func saveAndCloseEditor() {
        guard let chosenAppName = edAppDropdown.selectedItem?.title else { closeEditor(); return }
        let chosenAppId: String? = if chosenAppName == "Default" {
            nil
        } else {
            edAppDropdown.selectedItem?.representedObject as? String
        }

        // Build TargetEvent dict from editor state
        func buildEvent(isPress: Bool) -> [String: Any]? {
            let kf = isPress ? edPressKeyField : edHoldKeyField
            let md = isPress ? edPressMediaDropdown : edHoldMediaDropdown
            let gd = isPress ? edPressMouseDropdown : edHoldMouseDropdown
            let lbl = isPress ? edPressLabel : edHoldLabel
            let type = isPress ? pressType : holdType

            var evt: [String: Any] = ["type": type]
            if let l = lbl?.stringValue, !l.isEmpty { evt["label"] = l }

            switch type {
            case "Keyboard":
                if let b = kf?.stringValue, !b.isEmpty { evt["binding"] = b } else { return nil }
            case "MediaKey":
                let idx = md?.indexOfSelectedItem ?? 0
                guard idx >= 0, idx < mediaKeys.count else { return nil }
                evt["key_type"] = Int(mediaKeys[idx].keyType)
            case "MouseClick", "MouseScroll":
                let idx = gd?.indexOfSelectedItem ?? 0
                guard idx >= 0, idx < mouseGestures.count else { return nil }
                let mg = mouseGestures[idx]
                evt["type"] = mg.type
                if mg.type == "MouseClick" { evt["button"] = Int(mg.btn) }
                if mg.type == "MouseScroll" { evt["dx"] = 0.0; evt["dy"] = mg.dy }
            default: return nil
            }
            return evt
        }

        // Update config dict
        if chosenAppId == nil {
            // Default
            if var d = edConfig["default"] as? [String: Any] {
                if let evt = buildEvent(isPress: true) { d[edKey] = evt }
                if !edSingle, let holdKey = edKey.components(separatedBy: ",").last,
                   let evt = buildEvent(isPress: false) { d[holdKey] = evt }
                edConfig["default"] = d
            }
        } else {
            // Per-app profile
            var profiles = edConfig["profiles"] as? [[String: Any]] ?? []
            if let idx = profiles.firstIndex(where: { ($0["app_id"] as? String) == chosenAppId }) {
                if var m = profiles[idx]["mappings"] as? [String: Any] {
                    if let evt = buildEvent(isPress: true) { m[edKey] = evt }
                    if !edSingle, let holdKey = edKey.components(separatedBy: ",").last,
                       let evt = buildEvent(isPress: false) { m[holdKey] = evt }
                    profiles[idx]["mappings"] = m
                }
            } else {
                // Create new profile
                var m: [String: Any] = [:]
                if let evt = buildEvent(isPress: true) { m[edKey] = evt }
                if !edSingle, let holdKey = edKey.components(separatedBy: ",").last,
                   let evt = buildEvent(isPress: false) { m[holdKey] = evt }
                let newProfile: [String: Any] = [
                    "app_id": chosenAppId!,
                    "app_name": chosenAppName,
                    "mappings": m,
                ]
                profiles.append(newProfile)
            }
            edConfig["profiles"] = profiles
        }

        _ = writeConfig(edConfig)
        closeEditor()
    }
}

// ── App entry ────────────────────────────────────────────────────────────

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.setActivationPolicy(.accessory)
app.run()
