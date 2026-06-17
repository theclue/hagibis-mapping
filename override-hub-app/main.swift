import AppKit
import Foundation

@_silgen_name("hagibis_start") func hagibis_start() -> Int32
@_silgen_name("hagibis_stop") func hagibis_stop()
@_silgen_name("hagibis_is_running") func hagibis_is_running() -> Int32
@_silgen_name("hagibis_status_json") func hagibis_status_json(_ buf: UnsafeMutablePointer<CChar>, _ size: Int32) -> Int32
@_silgen_name("hagibis_elevate") func hagibis_elevate(_ path: UnsafePointer<CChar>) -> Int32

if geteuid() != 0 {
    if let execPath = Bundle.main.executablePath {
        _ = execPath.withCString { hagibis_elevate($0) }
    }
    exit(0)
}

// ── App Delegate (runs as root) ──────────────────────────────────────────
class AppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate {
    var statusItem: NSStatusItem!
    var timer: Timer?
    var window: NSWindow?
    var toggleMenuItem: NSMenuItem!

    var statusField: NSTextField!, appField: NSTextField!
    var btnTL: NSTextField!, btnTLh: NSTextField!, btnBR: NSTextField!, btnBRh: NSTextField!
    var mediaVolField: NSTextField!, mediaMuteField: NSTextField!, playField: NSTextField!

    let devTL = "Cmd+R", devTLh = "Ctrl+Q", devBR = "Cmd+F13", devBRh = "Ctrl+3"
    let dispW: CGFloat = 300

    func applicationDidFinishLaunching(_ n: Notification) {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        if let b = statusItem.button { b.title = "HM"; b.font = .monospacedDigitSystemFont(ofSize: 12, weight: .bold) }
        let m = NSMenu()
        toggleMenuItem = NSMenuItem(title: "Stop", action: #selector(toggleEngine), keyEquivalent: "")
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
            NSLog("[Hagibis Mapping] hagibis_start() failed — engine not running (check Library/Logs/override-hub)")
        }
        updateStatus()
    }
    @objc func stopEngine() { hagibis_stop(); updateStatus() }
    @objc func toggleEngine() { if hagibis_is_running() != 0 { stopEngine() } else { startEngine() } }

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
        let cons    = UInt8(truncatingIfNeeded: (s["consumer_sticky"] as? Int) ?? (s["consumer"] as? Int) ?? 0)
        let keys: [UInt8] = (s["keyboard_keys"] as? [Int])?.map { UInt8(truncatingIfNeeded: $0) } ?? []
        let lblTL   = s["btn_tl"] as? String ?? "—", lblTLh = s["btn_tl_hold"] as? String ?? "—"
        let lblBR   = s["btn_br"] as? String ?? "—", lblBRh = s["btn_br_hold"] as? String ?? "—"

        let isOn = { (code: UInt8) -> Bool in keys.contains(code) }
        let d = { (on: Bool) -> String in on ? "●" : "○" }

        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.statusItem?.button?.title = running ? "●HM" : "○HM"
            self.toggleMenuItem?.title = running ? "Stop" : "Start"
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
            self.mediaVolField?.stringValue = "\(d(cons & 0x01 != 0)) Vol+  \(d(cons & 0x02 != 0)) Vol−"
            self.mediaMuteField?.stringValue = "\(d(cons & 0x04 != 0)) Mute"
            self.playField?.stringValue = "\(d(cons & 0x40 != 0)) Play"
        }
    }

    @objc func showWindow() {
        if window == nil { createWindow() }
        NSApp.setActivationPolicy(.regular)
        window?.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
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
        // Load hub image: try bundle Resources first, then binary directory
        if let rp = Bundle.main.resourcePath {
            iv.image = NSImage(contentsOfFile: rp + "/hub.png")
        }
        if iv.image == nil {
            let execDir = URL(fileURLWithPath: CommandLine.arguments[0]).deletingLastPathComponent().path
            iv.image = NSImage(contentsOfFile: execDir + "/hub.png")
        }
        if iv.image == nil { iv.wantsLayer = true; iv.layer?.backgroundColor = NSColor.darkGray.cgColor }
        cv.addSubview(iv)

        let pw: CGFloat = 76 * s; let pad: Double = 0.10
        func ov(_ xf: Double, _ yf: Double, _ ra: Bool = false) -> NSTextField {
            let ox: CGFloat = ra ? CGFloat(1 - pad) * dispW - pw : CGFloat(xf) * dispW
            let oy = imgDH - CGFloat(yf) * imgDH - 14
            let bg = NSView(frame: NSRect(x: ox - 4, y: oy - 1, width: pw + 8, height: 18))
            bg.wantsLayer = true; bg.layer?.backgroundColor = NSColor(white: 1, alpha: 0.8).cgColor; bg.layer?.cornerRadius = 4
            iv.addSubview(bg)
            let f = NSTextField(labelWithString: "○ —")
            f.font = .monospacedDigitSystemFont(ofSize: 11, weight: .regular)
            f.textColor = NSColor(white: 0.15, alpha: 1)
            f.frame = NSRect(x: ox, y: oy, width: pw, height: 16)
            iv.addSubview(f); return f
        }
        btnTL = ov(pad, 0.12); btnTLh = ov(pad, 0.22)
        btnBR = ov(1 - pad, 0.78, true); btnBRh = ov(1 - pad, 0.88, true)
        mediaVolField = ov(0.38, 0.48); mediaMuteField = ov(0.38, 0.58)
        playField = ov(1 - pad, 0.12, true)

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

    @objc func showAbout() {
        NSApp.setActivationPolicy(.regular)
        NSApp.orderFrontStandardAboutPanel(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    @objc func quitApp() { timer?.invalidate(); timer = nil; hagibis_stop(); NSApp.terminate(nil) }
    func windowShouldClose(_ s: NSWindow) -> Bool { s.orderOut(nil); NSApp.setActivationPolicy(.accessory); return false }
}

let app = NSApplication.shared
// NSApplication.delegate is a WEAK property — keep a strong reference here, or
// ARC may deallocate the delegate and applicationDidFinishLaunching never fires.
let delegate = AppDelegate()
app.delegate = delegate
app.setActivationPolicy(.accessory)
app.run()
